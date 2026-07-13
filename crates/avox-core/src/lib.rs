//! `avox-core` — geteilte Domänentypen für Avox.
//!
//! Diese Kiste ist absichtlich abhängigkeitsfrei (nur `std`) und plattformneutral.
//! Sie definiert das gemeinsame Vokabular, das Engine, Service, IPC und GUI teilen.

use std::path::PathBuf;

/// Ergebnis eines Scans für einen einzelnen Pfad.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScanStatus {
    /// Keine Bedrohung gefunden.
    Clean,
    /// Bedrohung gefunden; enthält den Signaturnamen (z. B. `Eicar-Test-Signature`).
    Infected(String),
    /// Datei konnte nicht geprüft werden (Grund als Text).
    Error(String),
}

/// Ein einzelner Fund innerhalb eines Scans.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Betroffene Datei.
    pub path: PathBuf,
    /// Name der ausgelösten Signatur.
    pub signature: String,
}

/// Aggregiertes Ergebnis eines Scan-Laufs über einen oder mehrere Pfade.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScanReport {
    /// Anzahl geprüfter Dateien.
    pub scanned: u64,
    /// Alle gefundenen Bedrohungen.
    pub findings: Vec<Finding>,
    /// Nicht prüfbare Pfade mit Fehlertext.
    pub errors: Vec<(PathBuf, String)>,
}

impl ScanReport {
    /// `true`, wenn mindestens ein Fund vorliegt.
    pub fn is_infected(&self) -> bool {
        !self.findings.is_empty()
    }
}

/// Aktion, die auf einen Fund angewendet werden soll.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThreatAction {
    /// In die Quarantäne verschieben — sicherer Default (umkehrbar, löscht nichts).
    #[default]
    Quarantine,
    /// Endgültig löschen.
    Delete,
    /// Ignorieren / auf die Whitelist setzen.
    Ignore,
}

/// Adresse des `clamd`-Daemons, mit dem der Service spricht.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClamdAddress {
    /// Unix-Domain-Socket (Linux/macOS), z. B. `/var/run/clamav/clamd.ctl`.
    Unix(PathBuf),
    /// TCP-Endpunkt `host:port` (u. a. Windows), z. B. `127.0.0.1:3310`.
    Tcp(String),
}

impl Default for ClamdAddress {
    fn default() -> Self {
        ClamdAddress::Tcp("127.0.0.1:3310".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_flags_infection() {
        let mut r = ScanReport::default();
        assert!(!r.is_infected());
        r.findings.push(Finding {
            path: PathBuf::from("/tmp/x"),
            signature: "Eicar-Test-Signature".into(),
        });
        assert!(r.is_infected());
    }

    #[test]
    fn default_action_is_non_destructive() {
        assert_eq!(ThreatAction::default(), ThreatAction::Quarantine);
    }
}
