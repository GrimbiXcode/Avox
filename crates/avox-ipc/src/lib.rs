//! `avox-ipc` — Nachrichten-Vokabular zwischen dem privilegierten Avox-Service
//! und der unprivilegierten GUI.
//!
//! Transport (lokaler Unix-Socket / Named Pipe) und Serialisierung (JSON-RPC via
//! serde) werden in Meilenstein **M2** ergänzt. Diese Kiste definiert vorerst nur
//! die typisierten Request-/Response-Formen, damit Service und GUI gegen einen
//! stabilen Vertrag entwickeln können.

use std::path::PathBuf;

use avox_core::{ScanReport, ThreatAction};

/// Anfragen, die die GUI an den Service stellt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    /// Lebenszeichen / Verbindungstest.
    Ping,
    /// Version von Avox-Service und angebundenem clamd erfragen.
    GetVersion,
    /// Einen Pfad (Datei oder Ordner) scannen.
    Scan { path: PathBuf },
    /// Eine Aktion auf einen konkreten Fund anwenden.
    ApplyAction { path: PathBuf, action: ThreatAction },
    /// Signatur-Update (freshclam) anstoßen.
    UpdateSignatures,
}

/// Antworten des Service an die GUI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Response {
    /// Antwort auf [`Request::Ping`].
    Pong,
    /// Versionsangaben.
    Version { service: String, clamd: String },
    /// Ergebnis eines Scans.
    ScanResult(ScanReport),
    /// Aktion wurde ausgeführt.
    ActionApplied,
    /// Update-Ergebnis (z. B. „main.cvd aktualisiert").
    SignaturesUpdated { summary: String },
    /// Fehler mit menschenlesbarem Text.
    Error(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_roundtrip_shapes() {
        let r = Request::Scan {
            path: PathBuf::from("/home/user/Downloads"),
        };
        assert!(matches!(r, Request::Scan { .. }));
    }
}
