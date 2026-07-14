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
use std::path::PathBuf;
use std::time::Duration;

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
            ensure_service_running(app.handle());
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

/// Stellt sicher, dass der Avox-Dienst läuft. Auf **macOS** richtet die App dafür
/// selbst einen **launchd-LaunchAgent** ein (Autostart beim Login, unabhängig davon,
/// ob die GUI läuft); sonst wird die mitgelieferte Binary direkt gestartet.
/// Voraussetzung bleibt ein laufender `clamd`.
fn ensure_service_running(app: &tauri::AppHandle) {
    #[cfg(target_os = "macos")]
    {
        match ensure_launchagent(app) {
            Ok(()) => return,
            Err(e) => {
                eprintln!("Autostart konnte nicht eingerichtet werden ({e}) — starte Dienst direkt")
            }
        }
    }
    spawn_bundled_service(app);
}

/// Wartet kurz, bis der Dienst auf dem Socket lauscht.
fn wait_for_service() {
    for _ in 0..30 {
        if transport::connect(&endpoint()).is_ok() {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// Startet die mitgelieferte Dienst-Binary direkt (Fallback / Nicht-macOS).
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
            wait_for_service();
        }
        Err(e) => eprintln!(
            "Start von avox-service fehlgeschlagen ({}): {e}",
            bin.display()
        ),
    }
}

/// Richtet den launchd-LaunchAgent ein (kopiert die Binary an einen stabilen Ort,
/// schreibt die Plist, lädt sie idempotent). macOS-spezifisch.
#[cfg(target_os = "macos")]
fn ensure_launchagent(app: &tauri::AppHandle) -> std::io::Result<()> {
    use std::io;
    let home = std::env::var("HOME")
        .map_err(|_| io::Error::new(io::ErrorKind::NotFound, "HOME nicht gesetzt"))?;
    let home = PathBuf::from(home);

    // 1) Dienst-Binary an einen stabilen Ort kopieren (der Bundle-Pfad ändert sich
    //    bei App-Updates/-Verschieben; launchd braucht einen festen Pfad).
    let support = home.join("Library/Application Support/Avox");
    std::fs::create_dir_all(&support)?;
    let stable_bin = support.join("avox-service");
    let mut changed = false;
    match bundled_service_path(app) {
        Some(src) => changed |= copy_if_different(&src, &stable_bin)?,
        None if !stable_bin.exists() => {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "avox-service nicht gefunden",
            ));
        }
        None => {}
    }

    // 2) LaunchAgent-Plist schreiben.
    let agents = home.join("Library/LaunchAgents");
    std::fs::create_dir_all(&agents)?;
    let plist_path = agents.join("org.avox.service.plist");
    changed |= write_if_different(&plist_path, launchagent_plist(&stable_bin).as_bytes())?;

    // 3) Laden / bei Änderung neu laden (idempotent).
    let plist_str = plist_path.to_string_lossy().to_string();
    let loaded = launchctl_loaded();
    if changed || !loaded {
        if loaded {
            let _ = std::process::Command::new("launchctl")
                .args(["unload", &plist_str])
                .output();
        }
        let _ = std::process::Command::new("launchctl")
            .args(["load", "-w", &plist_str])
            .output();
        wait_for_service();
    }
    Ok(())
}

/// `true`, wenn der LaunchAgent aktuell geladen ist.
#[cfg(target_os = "macos")]
fn launchctl_loaded() -> bool {
    std::process::Command::new("launchctl")
        .args(["list", "org.avox.service"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Inhalt der LaunchAgent-Plist für den gegebenen Binary-Pfad.
#[cfg(target_os = "macos")]
fn launchagent_plist(bin: &std::path::Path) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>Label</key><string>org.avox.service</string>
  <key>ProgramArguments</key><array>
    <string>{}</string><string>serve</string>
  </array>
  <key>RunAtLoad</key><true/>
  <key>KeepAlive</key><true/>
  <key>StandardOutPath</key><string>/tmp/avox-service.log</string>
  <key>StandardErrorPath</key><string>/tmp/avox-service.log</string>
</dict></plist>
"#,
        bin.display()
    )
}

/// Kopiert `src` nach `dest`, wenn sich der Inhalt unterscheidet; setzt Ausführrecht.
/// Gibt `true` zurück, wenn kopiert wurde.
#[cfg(target_os = "macos")]
fn copy_if_different(src: &std::path::Path, dest: &std::path::Path) -> std::io::Result<bool> {
    let need = match (std::fs::read(src), std::fs::read(dest)) {
        (Ok(a), Ok(b)) => a != b,
        (Ok(_), Err(_)) => true,
        (Err(e), _) => return Err(e),
    };
    if need {
        std::fs::copy(src, dest)?;
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dest, std::fs::Permissions::from_mode(0o755))?;
    }
    Ok(need)
}

/// Schreibt `contents` nach `path`, wenn abweichend. Gibt `true` bei Änderung zurück.
#[cfg(target_os = "macos")]
fn write_if_different(path: &std::path::Path, contents: &[u8]) -> std::io::Result<bool> {
    let need = match std::fs::read(path) {
        Ok(existing) => existing != contents,
        Err(_) => true,
    };
    if need {
        std::fs::write(path, contents)?;
    }
    Ok(need)
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
