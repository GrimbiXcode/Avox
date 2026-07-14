//! Gemeinsame Scan-Logik, genutzt von IPC-Dispatch (Scan/FullScan) und Scheduler.

use std::io;
use std::path::PathBuf;

use avox_core::ScanReport;
use avox_engine::ClamdClient;

use crate::config::Config;

/// Ziel eines Scans.
pub enum ScanTarget {
    /// Ein konkreter Pfad (Datei oder Ordner).
    Path(PathBuf),
    /// Vollscan über die konfigurierten `full_scan_paths`.
    Full,
}

/// Führt den Scan aus und liefert ein (bei Vollscan aggregiertes) Ergebnis.
pub fn run(cfg: &Config, target: &ScanTarget) -> io::Result<ScanReport> {
    let client = ClamdClient::new(cfg.clamd.clone());
    match target {
        ScanTarget::Path(p) => client.scan_path(p),
        ScanTarget::Full => {
            let mut report = ScanReport::default();
            for path in &cfg.full_scan_paths {
                report.merge(client.scan_path(path)?);
            }
            Ok(report)
        }
    }
}
