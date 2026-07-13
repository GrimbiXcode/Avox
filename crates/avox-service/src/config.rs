//! Laufzeit-Konfiguration des Avox-Service, aufgelöst aus Umgebungsvariablen.
//!
//! In einer späteren Stufe kommt eine Konfigurationsdatei hinzu; das Vokabular
//! bleibt dabei dasselbe.

use std::env;
use std::path::PathBuf;

use avox_core::ClamdAddress;
use avox_ipc::transport::Endpoint;

/// Gebündelte Laufzeitparameter, an alle Verbindungs-Handler weitergereicht.
#[derive(Debug, Clone)]
pub struct Config {
    /// Adresse des clamd-Daemons.
    pub clamd: ClamdAddress,
    /// IPC-Endpoint, unter dem der Service lauscht.
    pub ipc: Endpoint,
    /// Verzeichnis für die Quarantäne.
    pub quarantine_dir: PathBuf,
    /// freshclam-Binary (Default: `freshclam` aus dem PATH).
    pub freshclam_bin: String,
    /// Optionale freshclam-Konfigurationsdatei.
    pub freshclam_conf: Option<PathBuf>,
}

impl Config {
    /// Liest die Konfiguration aus der Umgebung mit sinnvollen Defaults.
    pub fn from_env() -> Self {
        Config {
            clamd: clamd_from_env(),
            ipc: env::var("AVOX_IPC")
                .map(|s| Endpoint::parse(&s))
                .unwrap_or_else(|_| Endpoint::default_local()),
            quarantine_dir: env::var("AVOX_QUARANTINE_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|_| default_quarantine_dir()),
            freshclam_bin: env::var("AVOX_FRESHCLAM").unwrap_or_else(|_| "freshclam".to_string()),
            freshclam_conf: env::var("AVOX_FRESHCLAM_CONF").ok().map(PathBuf::from),
        }
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

/// Default-Quarantäneverzeichnis: `$HOME/.local/share/avox/quarantine`,
/// als Fallback ein Unterordner des System-Temp-Verzeichnisses.
fn default_quarantine_dir() -> PathBuf {
    if let Ok(home) = env::var("HOME") {
        PathBuf::from(home).join(".local/share/avox/quarantine")
    } else {
        env::temp_dir().join("avox-quarantine")
    }
}
