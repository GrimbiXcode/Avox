//! Zeitgesteuerte Scans.
//!
//! Pro konfiguriertem Zeitplan läuft ein Thread, der im Intervall einen Scan
//! auslöst, das Ergebnis protokolliert (`[sched]`) und Funde optional automatisch
//! in Quarantäne verschiebt. Standard ist **nur melden** (kein Auto-Löschen).

use std::thread;
use std::time::Duration;

use crate::config::{Config, ScheduleConfig};
use crate::quarantine::Quarantine;
use crate::scan::{self, ScanTarget};

/// Startet für jeden Zeitplan einen Hintergrund-Thread.
pub fn start(cfg: Config) {
    for sched in cfg.schedules.clone() {
        if sched.every_secs == 0 {
            eprintln!(
                "[sched] übersprungen (every_secs = 0): {}",
                sched.describe()
            );
            continue;
        }
        let cfg = cfg.clone();
        thread::spawn(move || run_loop(cfg, sched));
    }
}

fn run_loop(cfg: Config, sched: ScheduleConfig) {
    eprintln!(
        "[sched] aktiv: {} (alle {} s)",
        sched.describe(),
        sched.every_secs
    );
    let target = match target_for(&sched) {
        Some(t) => t,
        None => {
            eprintln!(
                "[sched] ungültig (kein Pfad, kein Vollscan): {}",
                sched.describe()
            );
            return;
        }
    };
    loop {
        thread::sleep(Duration::from_secs(sched.every_secs));
        eprintln!("[sched] starte: {}", sched.describe());
        match scan::run(&cfg, &target) {
            Ok(report) => {
                eprintln!(
                    "[sched] fertig: {} — geprüft {}, Funde {}",
                    sched.describe(),
                    report.scanned,
                    report.findings.len()
                );
                if sched.auto_quarantine && report.is_infected() {
                    quarantine_findings(&cfg, &report);
                }
            }
            Err(e) => eprintln!("[sched] Fehler bei {}: {e}", sched.describe()),
        }
    }
}

fn target_for(sched: &ScheduleConfig) -> Option<ScanTarget> {
    if sched.full {
        Some(ScanTarget::Full)
    } else {
        sched.path.clone().map(ScanTarget::Path)
    }
}

fn quarantine_findings(cfg: &Config, report: &avox_core::ScanReport) {
    let q = match Quarantine::new(&cfg.quarantine_dir) {
        Ok(q) => q,
        Err(e) => {
            eprintln!("[sched] Quarantäne-Verzeichnis nicht nutzbar: {e}");
            return;
        }
    };
    for f in &report.findings {
        match q.quarantine(&f.path) {
            Ok(entry) => eprintln!("[sched] in Quarantäne: {} ({})", f.path.display(), entry.id),
            Err(e) => eprintln!(
                "[sched] Quarantäne fehlgeschlagen für {}: {e}",
                f.path.display()
            ),
        }
    }
}
