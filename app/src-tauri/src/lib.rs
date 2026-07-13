//! Avox-GUI-Backend (Tauri v2).
//!
//! Die GUI ist **unprivilegiert** und spricht den privilegierten `avox-service`
//! ausschließlich über die IPC (`avox-ipc`) an — dieselbe Schnittstelle wie der
//! `call`-Client des Service. Jeder Tauri-Command öffnet eine kurzlebige
//! Verbindung, sendet eine Anfrage und liefert das Ergebnis an das Frontend.

use std::io::BufReader;

use avox_core::{QuarantineEntry, ScanReport, ThreatAction};
use avox_ipc::transport::{self, Endpoint};
use avox_ipc::{Request, RequestEnvelope, Response, ResponseEnvelope};
use serde::Serialize;

/// Versionsinfo für das Frontend.
#[derive(Serialize)]
pub struct VersionInfo {
    service: String,
    clamd: String,
}

/// IPC-Endpoint des Dienstes (überschreibbar via `AVOX_IPC`).
fn endpoint() -> Endpoint {
    std::env::var("AVOX_IPC")
        .map(|s| Endpoint::parse(&s))
        .unwrap_or_else(|_| Endpoint::default_local())
}

/// Sendet eine Anfrage an den Dienst und wartet auf die Antwort.
fn call(request: Request) -> Result<Response, String> {
    let conn = transport::connect(&endpoint()).map_err(|e| {
        format!("Avox-Dienst nicht erreichbar ({e}). Läuft `avox-service serve`?")
    })?;
    let mut reader = BufReader::new(conn);
    transport::write_msg(reader.get_mut(), &RequestEnvelope { id: 1, request })
        .map_err(|e| format!("Senden fehlgeschlagen: {e}"))?;
    match transport::read_msg::<_, ResponseEnvelope>(&mut reader)
        .map_err(|e| format!("Antwort fehlerhaft: {e}"))?
    {
        Some(env) => Ok(env.response),
        None => Err("keine Antwort vom Dienst".to_string()),
    }
}

/// Wandelt eine unerwartete Antwort in einen Fehlertext.
fn unexpected(r: &Response) -> String {
    match r {
        Response::Error(msg) => msg.clone(),
        other => format!("unerwartete Antwort: {other:?}"),
    }
}

#[tauri::command]
fn service_ping() -> Result<bool, String> {
    match call(Request::Ping)? {
        Response::Pong => Ok(true),
        other => Err(unexpected(&other)),
    }
}

#[tauri::command]
fn get_version() -> Result<VersionInfo, String> {
    match call(Request::GetVersion)? {
        Response::Version { service, clamd } => Ok(VersionInfo { service, clamd }),
        other => Err(unexpected(&other)),
    }
}

#[tauri::command]
fn scan(path: String) -> Result<ScanReport, String> {
    match call(Request::Scan { path: path.into() })? {
        Response::ScanResult(report) => Ok(report),
        other => Err(unexpected(&other)),
    }
}

#[tauri::command]
fn quarantine_file(path: String) -> Result<String, String> {
    apply(path, ThreatAction::Quarantine)
}

#[tauri::command]
fn delete_file(path: String) -> Result<String, String> {
    apply(path, ThreatAction::Delete)
}

fn apply(path: String, action: ThreatAction) -> Result<String, String> {
    match call(Request::ApplyAction {
        path: path.into(),
        action,
    })? {
        Response::ActionApplied { detail } => Ok(detail),
        other => Err(unexpected(&other)),
    }
}

#[tauri::command]
fn list_quarantine() -> Result<Vec<QuarantineEntry>, String> {
    match call(Request::ListQuarantine)? {
        Response::QuarantineList(entries) => Ok(entries),
        other => Err(unexpected(&other)),
    }
}

#[tauri::command]
fn restore(id: String) -> Result<String, String> {
    match call(Request::RestoreQuarantine { id })? {
        Response::ActionApplied { detail } => Ok(detail),
        other => Err(unexpected(&other)),
    }
}

#[tauri::command]
fn update_signatures() -> Result<String, String> {
    match call(Request::UpdateSignatures)? {
        Response::SignaturesUpdated { summary } => Ok(summary),
        other => Err(unexpected(&other)),
    }
}

/// Startet die Tauri-Anwendung.
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            service_ping,
            get_version,
            scan,
            quarantine_file,
            delete_file,
            list_quarantine,
            restore,
            update_signatures,
        ])
        .run(tauri::generate_context!())
        .expect("Fehler beim Starten der Avox-Anwendung");
}
