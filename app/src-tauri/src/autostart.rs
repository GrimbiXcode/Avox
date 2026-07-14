//! Plattformabhängiger Autostart für den Avox-Dienst und die ClamAV-Engine.
//!
//! - **macOS:** launchd-LaunchAgents (`~/Library/LaunchAgents`)
//! - **Linux:** systemd-User-Units (`~/.config/systemd/user`)
//! - **Windows:** geplante Aufgaben (`schtasks`, Trigger ONLOGON)
//!
//! Grundsatz: Schlägt die Einrichtung fehl, bleibt der App-Start unberührt — der
//! Aufrufer startet den Dienst zusätzlich direkt (Fallback in `lib.rs`).
//!
//! **avox-service** (Avox' eigener Dienst) wird auf allen Plattformen von der App
//! verwaltet. **clamd/freshclam** verwaltet die App nur auf macOS selbst (Homebrew
//! bringt keinen Dienst mit); auf Linux/Windows übernehmen das die
//! Distributions-/Installer-Dienste von ClamAV (Konvention) — läuft clamd nicht,
//! gibt es einen klaren Hinweis.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use avox_ipc::transport::{self, Endpoint};

/// TCP-Adresse, unter der clamd erwartet wird.
const CLAMD_ADDR: &str = "127.0.0.1:3310";

// ---------------------------------------------------------------------------
// Öffentliche API
// ---------------------------------------------------------------------------

/// Richtet den Autostart für den **Avox-Dienst** ein und stellt sicher, dass er
/// läuft. `bundled` ist die mitgelieferte `avox-service`-Binary (Bundle-Ressource).
/// Gibt `true` zurück, wenn der Dienst danach erreichbar ist; sonst soll der
/// Aufrufer die Binary direkt starten (Fallback).
pub fn ensure_avox_service(bundled: Option<&Path>) -> bool {
    // Binary an einen stabilen Ort kopieren (Bundle-Pfad ändert sich bei Updates).
    let Some(dir) = data_dir() else {
        return service_reachable();
    };
    let stable = dir.join(service_bin_name());
    // `true`, wenn die Binary gerade aktualisiert wurde → Autostart muss neu laden,
    // damit die neue Version auch wirklich läuft.
    let binary_changed = match bundled {
        Some(src) => copy_if_different(src, &stable).unwrap_or(false),
        None => false,
    };
    if !stable.exists() {
        return service_reachable();
    }

    install_avox_service_autostart(&stable, binary_changed);
    wait_for(service_reachable);
    service_reachable()
}

/// Richtet — wo nötig — den Autostart der ClamAV-Engine (clamd/freshclam) ein.
pub fn ensure_engine() {
    #[cfg(target_os = "macos")]
    macos_ensure_engine();

    #[cfg(not(target_os = "macos"))]
    if !clamd_reachable() {
        eprintln!(
            "clamd ist nicht erreichbar ({CLAMD_ADDR}). ClamAV-Dienst starten/aktivieren \
             (siehe README). Linux: `sudo systemctl enable --now clamav-daemon clamav-freshclam`. \
             Windows: ClamAV-Dienst über die Diensteverwaltung starten."
        );
    }
}

// ---------------------------------------------------------------------------
// Gemeinsame Helfer
// ---------------------------------------------------------------------------

fn service_endpoint() -> Endpoint {
    std::env::var("AVOX_IPC")
        .map(|s| Endpoint::parse(&s))
        .unwrap_or_else(|_| Endpoint::default_local())
}

fn service_reachable() -> bool {
    transport::connect(&service_endpoint()).is_ok()
}

#[allow(dead_code)]
fn clamd_reachable() -> bool {
    std::net::TcpStream::connect(CLAMD_ADDR).is_ok()
}

fn wait_for<F: Fn() -> bool>(check: F) {
    for _ in 0..30 {
        if check() {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn service_bin_name() -> &'static str {
    if cfg!(windows) {
        "avox-service.exe"
    } else {
        "avox-service"
    }
}

/// Home-Verzeichnis (`HOME` bzw. `USERPROFILE`).
#[cfg_attr(windows, allow(dead_code))]
fn home() -> Option<PathBuf> {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .ok()
        .map(PathBuf::from)
}

/// Stabiler Ablageort für die kopierte Dienst-Binary (pro Plattform üblich).
fn data_dir() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        home().map(|h| h.join("Library/Application Support/Avox"))
    }
    #[cfg(target_os = "windows")]
    {
        std::env::var("LOCALAPPDATA")
            .ok()
            .map(|p| PathBuf::from(p).join("Avox"))
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        home().map(|h| h.join(".local/share/avox"))
    }
}

/// Kopiert `src` nach `dest`, wenn abweichend; setzt (unix) das Ausführrecht.
fn copy_if_different(src: &Path, dest: &Path) -> std::io::Result<bool> {
    let need = match (std::fs::read(src), std::fs::read(dest)) {
        (Ok(a), Ok(b)) => a != b,
        (Ok(_), Err(_)) => true,
        (Err(e), _) => return Err(e),
    };
    if need {
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(src, dest)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(dest, std::fs::Permissions::from_mode(0o755))?;
        }
    }
    Ok(need)
}

// ---------------------------------------------------------------------------
// macOS: launchd
// ---------------------------------------------------------------------------

#[cfg(target_os = "macos")]
enum Schedule {
    KeepAlive,
    Interval(u32),
}

#[cfg(target_os = "macos")]
fn install_avox_service_autostart(bin: &Path, force_reload: bool) -> bool {
    launchd_install(
        "org.avox.service",
        &[bin.to_string_lossy().into_owned(), "serve".into()],
        Schedule::KeepAlive,
        "/tmp/avox-service.log",
        force_reload,
    )
}

#[cfg(target_os = "macos")]
fn macos_ensure_engine() {
    if !launchctl_is_loaded("org.clamav.clamd") {
        let bin = find_executable(
            "clamd",
            &[
                "/opt/homebrew/sbin",
                "/usr/local/sbin",
                "/usr/sbin",
                "/usr/bin",
            ],
        );
        match (bin, find_clamav_config("clamd.conf")) {
            (Some(bin), Some(conf)) => {
                launchd_install(
                    "org.clamav.clamd",
                    &[
                        bin.to_string_lossy().into_owned(),
                        "--foreground".into(),
                        format!("--config-file={}", conf.display()),
                    ],
                    Schedule::KeepAlive,
                    "/tmp/clamd.log",
                    false,
                );
            }
            _ => eprintln!(
                "clamd/clamd.conf nicht gefunden — ClamAV installieren/konfigurieren (siehe README)"
            ),
        }
    }
    if !launchctl_is_loaded("org.clamav.freshclam") {
        let bin = find_executable(
            "freshclam",
            &["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin"],
        );
        match (bin, find_clamav_config("freshclam.conf")) {
            (Some(bin), Some(conf)) => {
                launchd_install(
                    "org.clamav.freshclam",
                    &[
                        bin.to_string_lossy().into_owned(),
                        format!("--config-file={}", conf.display()),
                    ],
                    Schedule::Interval(21600),
                    "/tmp/freshclam.log",
                    false,
                );
            }
            _ => eprintln!(
                "freshclam/freshclam.conf nicht gefunden — ClamAV installieren/konfigurieren (siehe README)"
            ),
        }
    }
}

#[cfg(target_os = "macos")]
fn launchctl_is_loaded(label: &str) -> bool {
    Command::new("launchctl")
        .args(["list", label])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Schreibt eine LaunchAgent-Plist und lädt sie. Neu geladen wird nur, wenn die
/// Plist sich geändert hat, `force_reload` gesetzt ist (z. B. aktualisierte Binary)
/// oder der Agent noch nicht läuft. Gibt `true` bei (vermutetem) Erfolg zurück.
#[cfg(target_os = "macos")]
fn launchd_install(
    label: &str,
    args: &[String],
    schedule: Schedule,
    log: &str,
    force_reload: bool,
) -> bool {
    let Some(home) = home() else {
        return false;
    };
    let agents = home.join("Library/LaunchAgents");
    if std::fs::create_dir_all(&agents).is_err() {
        return false;
    }
    let plist_path = agents.join(format!("{label}.plist"));
    let content = launchd_plist(label, args, &schedule, log);
    let plist_changed = match write_if_different(&plist_path, content.as_bytes()) {
        Ok(changed) => changed,
        Err(_) => return false,
    };
    let loaded = launchctl_is_loaded(label);
    if plist_changed || force_reload || !loaded {
        let plist_str = plist_path.to_string_lossy();
        if loaded {
            let _ = Command::new("launchctl")
                .args(["unload", &plist_str])
                .output();
        }
        let _ = Command::new("launchctl")
            .args(["load", "-w", &plist_str])
            .output();
        eprintln!("LaunchAgent eingerichtet/aktualisiert: {label}");
    }
    true
}

/// Schreibt `contents` nach `path`, wenn abweichend. Gibt `true` bei Änderung zurück.
#[cfg(target_os = "macos")]
fn write_if_different(path: &Path, contents: &[u8]) -> std::io::Result<bool> {
    let need = match std::fs::read(path) {
        Ok(existing) => existing != contents,
        Err(_) => true,
    };
    if need {
        std::fs::write(path, contents)?;
    }
    Ok(need)
}

#[cfg(target_os = "macos")]
fn launchd_plist(label: &str, args: &[String], schedule: &Schedule, log: &str) -> String {
    let args_xml: String = args
        .iter()
        .map(|a| format!("    <string>{}</string>\n", xml_escape(a)))
        .collect();
    let schedule_xml = match schedule {
        Schedule::KeepAlive => {
            "  <key>RunAtLoad</key><true/>\n  <key>KeepAlive</key><true/>".to_string()
        }
        Schedule::Interval(secs) => format!(
            "  <key>RunAtLoad</key><true/>\n  <key>StartInterval</key><integer>{secs}</integer>"
        ),
    };
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>Label</key><string>{label}</string>
  <key>ProgramArguments</key><array>
{args_xml}  </array>
{schedule_xml}
  <key>StandardOutPath</key><string>{log}</string>
  <key>StandardErrorPath</key><string>{log}</string>
</dict></plist>
"#
    )
}

#[cfg(target_os = "macos")]
fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(target_os = "macos")]
fn find_executable(name: &str, dirs: &[&str]) -> Option<PathBuf> {
    for dir in dirs {
        let p = Path::new(dir).join(name);
        if p.exists() {
            return Some(p);
        }
    }
    let out = Command::new("which").arg(name).output().ok()?;
    if out.status.success() {
        let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if !s.is_empty() {
            return Some(PathBuf::from(s));
        }
    }
    None
}

#[cfg(target_os = "macos")]
fn find_clamav_config(filename: &str) -> Option<PathBuf> {
    for dir in [
        "/opt/homebrew/etc/clamav",
        "/usr/local/etc/clamav",
        "/etc/clamav",
        "/opt/homebrew/etc",
        "/usr/local/etc",
    ] {
        let p = Path::new(dir).join(filename);
        if p.exists() {
            return Some(p);
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Linux: systemd-User-Unit
// ---------------------------------------------------------------------------

#[cfg(all(unix, not(target_os = "macos")))]
fn install_avox_service_autostart(bin: &Path, force_reload: bool) -> bool {
    let Some(home) = home() else {
        return false;
    };
    let unit_dir = home.join(".config/systemd/user");
    if std::fs::create_dir_all(&unit_dir).is_err() {
        return false;
    }
    let unit = format!(
        "[Unit]\n\
         Description=Avox Antivirus Service\n\
         After=network.target\n\n\
         [Service]\n\
         Type=simple\n\
         ExecStart={} serve\n\
         Restart=on-failure\n\n\
         [Install]\n\
         WantedBy=default.target\n",
        bin.display()
    );
    let unit_path = unit_dir.join("avox-service.service");
    if std::fs::write(&unit_path, unit).is_err() {
        return false;
    }
    let _ = Command::new("systemctl")
        .args(["--user", "daemon-reload"])
        .output();
    let ok = Command::new("systemctl")
        .args(["--user", "enable", "--now", "avox-service.service"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if ok && force_reload {
        // Aktualisierte Binary aktiv werden lassen.
        let _ = Command::new("systemctl")
            .args(["--user", "restart", "avox-service.service"])
            .output();
    }
    if ok {
        eprintln!("systemd-User-Unit eingerichtet: avox-service.service");
    }
    ok
}

// ---------------------------------------------------------------------------
// Windows: geplante Aufgabe (schtasks, ONLOGON)
// ---------------------------------------------------------------------------

#[cfg(windows)]
fn install_avox_service_autostart(bin: &Path, _force_reload: bool) -> bool {
    // Die Aufgabe wird ohnehin per /f neu erstellt und per /run gestartet, daher
    // ist eine gesonderte Reload-Behandlung nicht nötig.
    let tr = format!("\"{}\" serve", bin.display());
    // /f überschreibt eine bestehende Aufgabe; /rl LIMITED = normale Nutzerrechte.
    let created = Command::new("schtasks")
        .args([
            "/create",
            "/tn",
            "AvoxService",
            "/tr",
            &tr,
            "/sc",
            "ONLOGON",
            "/rl",
            "LIMITED",
            "/f",
        ])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !created {
        return false;
    }
    // Sofort einmal starten (die Aufgabe würde sonst erst beim nächsten Login laufen).
    let _ = Command::new("schtasks")
        .args(["/run", "/tn", "AvoxService"])
        .output();
    eprintln!("Geplante Aufgabe eingerichtet: AvoxService (ONLOGON)");
    true
}
