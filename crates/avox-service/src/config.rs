//! Laufzeit-Konfiguration des Avox-Service.
//!
//! Basis sind Umgebungsvariablen (schnelle Overrides); Zeitpläne und Vollscan-Pfade
//! kommen aus einer optionalen JSON-**Konfigurationsdatei** (`AVOX_CONFIG`, Default
//! `$HOME/.config/avox/config.json`). Fehlt die Datei, gelten leere/Standardwerte.

use std::env;
use std::path::PathBuf;

use avox_core::{ClamdAddress, ScheduleInfo};
use serde::{Deserialize, Serialize};

/// Ein konfigurierter Zeitplan (aus der Config-Datei).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduleConfig {
    /// Intervall in Sekunden.
    pub every_secs: u64,
    /// `true` = Vollscan der `full_scan_paths`; sonst gezielter `path`-Scan.
    #[serde(default)]
    pub full: bool,
    /// Zu scannender Pfad (nur bei `full = false`).
    #[serde(default)]
    pub path: Option<PathBuf>,
    /// Funde automatisch in Quarantäne verschieben (Default: nur melden).
    #[serde(default)]
    pub auto_quarantine: bool,
    /// Optionale Bezeichnung für die Anzeige.
    #[serde(default)]
    pub label: Option<String>,
}

impl ScheduleConfig {
    /// Menschlesbare Beschreibung für die GUI.
    pub fn describe(&self) -> String {
        if let Some(l) = &self.label {
            return l.clone();
        }
        let what = if self.full {
            "Vollscan".to_string()
        } else {
            match &self.path {
                Some(p) => format!("Scan von {}", p.display()),
                None => "Scan".to_string(),
            }
        };
        format!("{what} alle {}", humanize(self.every_secs))
    }
}

/// Struktur der Konfigurationsdatei.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FileConfig {
    #[serde(default)]
    pub schedules: Vec<ScheduleConfig>,
    #[serde(default)]
    pub full_scan_paths: Vec<PathBuf>,
}

/// Gebündelte Laufzeitparameter, an alle Verbindungs-Handler weitergereicht.
#[derive(Debug, Clone)]
pub struct Config {
    pub clamd: ClamdAddress,
    pub ipc: avox_ipc::transport::Endpoint,
    pub quarantine_dir: PathBuf,
    pub freshclam_bin: String,
    pub freshclam_conf: Option<PathBuf>,
    /// Konfigurierte Zeitpläne.
    pub schedules: Vec<ScheduleConfig>,
    /// Pfade für den Vollscan (Default: Home-Verzeichnis).
    pub full_scan_paths: Vec<PathBuf>,
}

impl Config {
    /// Liest die Konfiguration aus Umgebung + Config-Datei mit sinnvollen Defaults.
    pub fn from_env() -> Self {
        let file = load_file_config();
        let full_scan_paths = if file.full_scan_paths.is_empty() {
            vec![default_home()]
        } else {
            file.full_scan_paths
        };
        Config {
            clamd: clamd_from_env(),
            ipc: env::var("AVOX_IPC")
                .map(|s| avox_ipc::transport::Endpoint::parse(&s))
                .unwrap_or_else(|_| avox_ipc::transport::Endpoint::default_local()),
            quarantine_dir: env::var("AVOX_QUARANTINE_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|_| default_quarantine_dir()),
            freshclam_bin: env::var("AVOX_FRESHCLAM").unwrap_or_else(|_| default_freshclam_bin()),
            freshclam_conf: env::var("AVOX_FRESHCLAM_CONF")
                .ok()
                .map(PathBuf::from)
                .or_else(default_freshclam_conf),
            schedules: file.schedules,
            full_scan_paths,
        }
    }

    /// Zeitpläne als GUI-taugliche Info-Liste.
    pub fn schedule_infos(&self) -> Vec<ScheduleInfo> {
        self.schedules
            .iter()
            .map(|s| ScheduleInfo {
                description: s.describe(),
                every_secs: s.every_secs,
                full: s.full,
            })
            .collect()
    }
}

/// Lädt die Config-Datei, falls vorhanden. Fehler/kein File → Defaults.
fn load_file_config() -> FileConfig {
    let path = env::var("AVOX_CONFIG")
        .map(PathBuf::from)
        .unwrap_or_else(|_| default_config_path());
    match std::fs::read_to_string(&path) {
        Ok(content) => match serde_json::from_str::<FileConfig>(&content) {
            Ok(cfg) => cfg,
            Err(e) => {
                eprintln!(
                    "Config {} ungültig ({e}) — Defaults werden verwendet",
                    path.display()
                );
                FileConfig::default()
            }
        },
        Err(_) => FileConfig::default(),
    }
}

/// `AVOX_CLAMD_ADDR`: `host:port` → TCP, sonst Unix-Socket-Pfad; Default 127.0.0.1:3310.
pub fn clamd_from_env() -> ClamdAddress {
    match env::var("AVOX_CLAMD_ADDR") {
        Ok(v) if v.contains(':') && !v.starts_with('/') => ClamdAddress::Tcp(v),
        Ok(v) => ClamdAddress::Unix(PathBuf::from(v)),
        Err(_) => ClamdAddress::default(),
    }
}

fn default_home() -> PathBuf {
    env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| env::temp_dir())
}

fn default_config_path() -> PathBuf {
    default_home().join(".config/avox/config.json")
}

fn default_quarantine_dir() -> PathBuf {
    if let Ok(home) = env::var("HOME") {
        PathBuf::from(home).join(".local/share/avox/quarantine")
    } else {
        env::temp_dir().join("avox-quarantine")
    }
}

/// Ermittelt die `freshclam`-Binary. Der Dienst läuft oft unter launchd/systemd mit
/// minimalem `PATH`, daher suchen wir absolute Standardpfade, bevor wir auf den
/// Namen (PATH-Auflösung) zurückfallen.
fn default_freshclam_bin() -> String {
    let candidates = [
        "/opt/homebrew/bin/freshclam",
        "/usr/local/bin/freshclam",
        "/usr/bin/freshclam",
        "/opt/local/bin/freshclam",
        r"C:\Program Files\ClamAV\freshclam.exe",
    ];
    for c in candidates {
        if std::path::Path::new(c).exists() {
            return c.to_string();
        }
    }
    "freshclam".to_string() // Fallback: über PATH
}

/// Sucht eine vorhandene `freshclam.conf` in den üblichen Verzeichnissen.
fn default_freshclam_conf() -> Option<PathBuf> {
    let candidates = [
        "/opt/homebrew/etc/clamav/freshclam.conf",
        "/usr/local/etc/clamav/freshclam.conf",
        "/etc/clamav/freshclam.conf",
        "/opt/local/etc/clamav/freshclam.conf",
        r"C:\Program Files\ClamAV\freshclam.conf",
    ];
    candidates.iter().map(PathBuf::from).find(|p| p.exists())
}

/// Formatiert ein Intervall grob menschlesbar (Tage/Stunden/Minuten/Sekunden).
fn humanize(secs: u64) -> String {
    match secs {
        s if s % 86400 == 0 && s >= 86400 => format!("{} Tag(e)", s / 86400),
        s if s % 3600 == 0 && s >= 3600 => format!("{} h", s / 3600),
        s if s % 60 == 0 && s >= 60 => format!("{} min", s / 60),
        s => format!("{s} s"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn describe_uses_label_then_falls_back() {
        let s = ScheduleConfig {
            every_secs: 86400,
            full: true,
            path: None,
            auto_quarantine: false,
            label: None,
        };
        assert_eq!(s.describe(), "Vollscan alle 1 Tag(e)");

        let s2 = ScheduleConfig {
            every_secs: 3600,
            full: false,
            path: Some(PathBuf::from("/tmp/x")),
            auto_quarantine: false,
            label: Some("Stündlicher Downloads-Scan".into()),
        };
        assert_eq!(s2.describe(), "Stündlicher Downloads-Scan");
    }

    #[test]
    fn humanize_intervals() {
        assert_eq!(humanize(86400), "1 Tag(e)");
        assert_eq!(humanize(7200), "2 h");
        assert_eq!(humanize(300), "5 min");
        assert_eq!(humanize(45), "45 s");
    }
}
