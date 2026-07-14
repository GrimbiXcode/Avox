//! `avox-ipc` — Nachrichten-Vertrag **und** Transport zwischen dem privilegierten
//! Avox-Service und der unprivilegierten GUI.
//!
//! Transport: lokaler **Unix-Domain-Socket** (Linux/macOS) bzw. **loopback-TCP**
//! als Fallback (u. a. Windows, bis ein Named-Pipe-Transport in einer späteren
//! Stufe folgt). Framing: **line-delimited JSON** (ein JSON-Objekt pro Zeile) —
//! einfach, debugbar (`nc -U`), streamfähig.
//!
//! Jede Nachricht trägt eine `id` zur Korrelation von Anfrage und Antwort.

use std::path::PathBuf;

use avox_core::{QuarantineEntry, ScanReport, ScheduleInfo, ThreatAction};
use serde::{Deserialize, Serialize};

pub mod transport;

/// Anfragen, die die GUI an den Service stellt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Request {
    /// Lebenszeichen / Verbindungstest.
    Ping,
    /// Version von Avox-Service und angebundenem clamd erfragen.
    GetVersion,
    /// Einen Pfad (Datei oder Ordner) scannen.
    Scan { path: PathBuf },
    /// Vollständiger Scan der konfigurierten Pfade (Default: Home-Verzeichnis).
    FullScan,
    /// Die konfigurierten Zeitpläne abfragen (für die Anzeige in der GUI).
    GetSchedule,
    /// Eine Aktion auf einen konkreten Fund anwenden.
    ApplyAction { path: PathBuf, action: ThreatAction },
    /// Inhalt der Quarantäne auflisten.
    ListQuarantine,
    /// Eine Datei aus der Quarantäne an ihren Ursprungsort zurückstellen.
    RestoreQuarantine { id: String },
    /// Signatur-Update (freshclam) anstoßen.
    UpdateSignatures,
}

/// Antworten des Service an die GUI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Response {
    /// Antwort auf [`Request::Ping`].
    Pong,
    /// Versionsangaben.
    Version { service: String, clamd: String },
    /// Ergebnis eines Scans.
    ScanResult(ScanReport),
    /// Aktion wurde ausgeführt (mit kurzer Beschreibung, z. B. Quarantäne-Pfad).
    ActionApplied { detail: String },
    /// Inhalt der Quarantäne.
    QuarantineList(Vec<QuarantineEntry>),
    /// Konfigurierte Zeitpläne.
    Schedule(Vec<ScheduleInfo>),
    /// Update-Ergebnis (z. B. „daily.cvd aktualisiert").
    SignaturesUpdated { summary: String },
    /// Fehler mit menschenlesbarem Text.
    Error(String),
}

/// Anfrage-Umschlag mit Korrelations-ID.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestEnvelope {
    pub id: u64,
    pub request: Request,
}

/// Antwort-Umschlag mit derselben Korrelations-ID wie die Anfrage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResponseEnvelope {
    pub id: u64,
    pub response: Response,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_envelope_json_roundtrip() {
        let env = RequestEnvelope {
            id: 42,
            request: Request::Scan {
                path: PathBuf::from("/home/user/Downloads"),
            },
        };
        let line = serde_json::to_string(&env).unwrap();
        let back: RequestEnvelope = serde_json::from_str(&line).unwrap();
        assert_eq!(env, back);
    }

    #[test]
    fn response_envelope_json_roundtrip() {
        let env = ResponseEnvelope {
            id: 42,
            response: Response::Version {
                service: "0.0.1".into(),
                clamd: "ClamAV 1.5.2".into(),
            },
        };
        let line = serde_json::to_string(&env).unwrap();
        let back: ResponseEnvelope = serde_json::from_str(&line).unwrap();
        assert_eq!(env, back);
    }
}
