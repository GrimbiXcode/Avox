//! Avox-GUI-Backend (Tauri v2).
//!
//! Die GUI ist **unprivilegiert** und spricht den privilegierten `avox-service`
//! ausschließlich über die IPC (`avox-ipc`) an — dieselbe Schnittstelle wie der
//! `call`-Client des Service.
//!
//! Alle Commands sind **async** und führen die blockierende Socket-IO über
//! `spawn_blocking` aus. Dadurch bleibt der UI-Thread frei — selbst wenn der
//! Dienst hängt oder ein Scan lange dauert, friert das Fenster nicht ein.
//!
//! Beim Start richtet die App den Autostart des Dienstes (und, wo nötig, der
//! ClamAV-Engine) plattformabhängig ein — siehe [`autostart`].

use std::io::BufReader;
use std::path::PathBuf;
use std::time::Duration;

use avox_core::{QuarantineEntry, ScanReport, ScheduleInfo, ThreatAction};
use avox_ipc::transport::{self, Endpoint};
use avox_ipc::{Request, RequestEnvelope, Response, ResponseEnvelope};
use serde::Serialize;
use tauri::Manager;

mod autostart;

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
        .map_err(|e| format!("Avox-Dienst nicht erreichbar ({e}). Läuft der Dienst?"))?;
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
            ensure_services(app.handle());
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

/// Stellt beim Start sicher, dass der Avox-Dienst (und, wo nötig, die ClamAV-Engine)
/// läuft — plattformabhängig via [`autostart`]. Scheitert die Autostart-Einrichtung,
/// wird die mitgelieferte Dienst-Binary direkt gestartet (Fallback).
fn ensure_services(app: &tauri::AppHandle) {
    let bundled = bundled_service_path(app);
    if !autostart::ensure_avox_service(bundled.as_deref()) {
        spawn_bundled_service(app);
    }
    autostart::ensure_engine();
}

/// Startet die mitgelieferte Dienst-Binary direkt (Fallback, wenn kein Autostart
/// eingerichtet werden konnte).
fn spawn_bundled_service(app: &tauri::AppHandle) {
    if transport::connect(&endpoint()).is_ok() {
        return;
    }
    let Some(bin) = bundled_service_path(app) else {
        eprintln!("avox-service nicht gefunden — bitte manuell `avox-service serve` starten");
        return;
    };
    match std::process::Command::new(&bin).arg("serve").spawn() {
        Ok(_) => {
            eprintln!("avox-service gestartet: {}", bin.display());
            for _ in 0..30 {
                if transport::connect(&endpoint()).is_ok() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        }
        Err(e) => eprintln!(
            "Start von avox-service fehlgeschlagen ({}): {e}",
            bin.display()
        ),
    }
}

/// Sucht die mitgelieferte `avox-service`-Binary (Bundle-Ressource; Dev-Fallback
/// neben der App-Binary).
fn bundled_service_path(app: &tauri::AppHandle) -> Option<PathBuf> {
    let name = if cfg!(windows) {
        "avox-service.exe"
    } else {
        "avox-service"
    };
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(res) = app.path().resource_dir() {
        candidates.push(res.join("resources").join(name));
        candidates.push(res.join(name));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join(name));
        }
    }
    candidates.into_iter().find(|p| p.exists())
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
