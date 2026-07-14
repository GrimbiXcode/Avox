//! Avox-GUI-Backend (Tauri v2).
//!
//! Die GUI ist **unprivilegiert** und spricht den privilegierten `avox-service`
//! ausschließlich über die IPC (`avox-ipc`) an — dieselbe Schnittstelle wie der
//! `call`-Client des Service.
//!
//! Alle Commands sind **async** und führen die blockierende Socket-IO über
//! `spawn_blocking` aus. Dadurch bleibt der UI-Thread frei — selbst wenn der
//! Dienst hängt oder ein Scan lange dauert, friert das Fenster nicht ein.

use std::io::BufReader;

use avox_core::{QuarantineEntry, ScanReport, ScheduleInfo, ThreatAction};
use avox_ipc::transport::{self, Endpoint};
use avox_ipc::{Request, RequestEnvelope, Response, ResponseEnvelope};
use serde::Serialize;
use tauri::Manager;

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

/// Blockierender IPC-Aufruf (läuft nur innerhalb von `spawn_blocking`).
fn call_blocking(request: Request) -> Result<Response, String> {
    let conn = transport::connect(&endpoint())
        .map_err(|e| format!("Avox-Dienst nicht erreichbar ({e}). Läuft `avox-service serve`?"))?;
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

/// Führt den blockierenden Aufruf auf einem Blocking-Thread aus, ohne den
/// UI-/Runtime-Thread zu blockieren.
async fn call(request: Request) -> Result<Response, String> {
    match tauri::async_runtime::spawn_blocking(move || call_blocking(request)).await {
        Ok(result) => result,
        Err(e) => Err(format!("interner Fehler beim IPC-Aufruf: {e}")),
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
async fn service_ping() -> Result<bool, String> {
    match call(Request::Ping).await? {
        Response::Pong => Ok(true),
        other => Err(unexpected(&other)),
    }
}

#[tauri::command]
async fn get_version() -> Result<VersionInfo, String> {
    match call(Request::GetVersion).await? {
        Response::Version { service, clamd } => Ok(VersionInfo { service, clamd }),
        other => Err(unexpected(&other)),
    }
}

#[tauri::command]
async fn scan(path: String) -> Result<ScanReport, String> {
    match call(Request::Scan { path: path.into() }).await? {
        Response::ScanResult(report) => Ok(report),
        other => Err(unexpected(&other)),
    }
}

#[tauri::command]
async fn full_scan() -> Result<ScanReport, String> {
    match call(Request::FullScan).await? {
        Response::ScanResult(report) => Ok(report),
        other => Err(unexpected(&other)),
    }
}

#[tauri::command]
async fn get_schedule() -> Result<Vec<ScheduleInfo>, String> {
    match call(Request::GetSchedule).await? {
        Response::Schedule(items) => Ok(items),
        other => Err(unexpected(&other)),
    }
}

#[tauri::command]
async fn quarantine_file(path: String) -> Result<String, String> {
    apply(path, ThreatAction::Quarantine).await
}

#[tauri::command]
async fn delete_file(path: String) -> Result<String, String> {
    apply(path, ThreatAction::Delete).await
}

async fn apply(path: String, action: ThreatAction) -> Result<String, String> {
    match call(Request::ApplyAction {
        path: path.into(),
        action,
    })
    .await?
    {
        Response::ActionApplied { detail } => Ok(detail),
        other => Err(unexpected(&other)),
    }
}

#[tauri::command]
async fn list_quarantine() -> Result<Vec<QuarantineEntry>, String> {
    match call(Request::ListQuarantine).await? {
        Response::QuarantineList(entries) => Ok(entries),
        other => Err(unexpected(&other)),
    }
}

#[tauri::command]
async fn restore(id: String) -> Result<String, String> {
    match call(Request::RestoreQuarantine { id }).await? {
        Response::ActionApplied { detail } => Ok(detail),
        other => Err(unexpected(&other)),
    }
}

#[tauri::command]
async fn update_signatures() -> Result<String, String> {
    match call(Request::UpdateSignatures).await? {
        Response::SignaturesUpdated { summary } => Ok(summary),
        other => Err(unexpected(&other)),
    }
}

/// Startet die Tauri-Anwendung inkl. System-Tray.
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            build_tray(app.handle())?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            service_ping,
            get_version,
            scan,
            full_scan,
            get_schedule,
            quarantine_file,
            delete_file,
            list_quarantine,
            restore,
            update_signatures,
        ])
        .run(tauri::generate_context!())
        .expect("Fehler beim Starten der Avox-Anwendung");
}

/// Baut das System-Tray-Icon mit Kontextmenü (Öffnen / Beenden).
fn build_tray(app: &tauri::AppHandle) -> tauri::Result<()> {
    use tauri::menu::{Menu, MenuItem};
    use tauri::tray::TrayIconBuilder;

    let open = MenuItem::with_id(app, "open", "Avox öffnen", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Beenden", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &quit])?;

    let mut builder = TrayIconBuilder::new()
        .tooltip("Avox Antivirus")
        .menu(&menu)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => show_main(app),
            "quit" => app.exit(0),
            _ => {}
        });
    if let Some(icon) = app.default_window_icon().cloned() {
        builder = builder.icon(icon);
    }
    builder.build(app)?;
    Ok(())
}

/// Zeigt das Hauptfenster und holt es in den Vordergrund.
fn show_main(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}
