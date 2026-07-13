//! `avox-service` — privilegierter Hintergrunddienst von Avox.
//!
//! **Skelett-Stand (M0/M1):** Diese Binärdatei zeigt bereits den vollständigen
//! Pfad GUI → Service → clamd anhand einer kleinen CLI. Der echte IPC-Server
//! (lokaler Socket, JSON-RPC), Zeitpläne und Quarantäne folgen in M2/M3.
//!
//! Verwendung:
//!   avox-service ping                 # Verbindung zu clamd testen
//!   avox-service version              # clamd-Version anzeigen
//!   avox-service scan <PFAD>          # Pfad scannen
//!
//! clamd-Adresse via Umgebungsvariable `AVOX_CLAMD_ADDR` überschreibbar
//! (z. B. `127.0.0.1:3310` oder ein Unix-Socket-Pfad).

use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

use avox_core::ClamdAddress;
use avox_engine::ClamdClient;

fn resolve_addr() -> ClamdAddress {
    match env::var("AVOX_CLAMD_ADDR") {
        Ok(v) if v.contains(':') && !v.starts_with('/') => ClamdAddress::Tcp(v),
        Ok(v) => ClamdAddress::Unix(PathBuf::from(v)),
        Err(_) => ClamdAddress::default(),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let client = ClamdClient::new(resolve_addr());

    match args.first().map(String::as_str) {
        Some("ping") => match client.ping() {
            Ok(true) => {
                println!("clamd: PONG (erreichbar)");
                ExitCode::SUCCESS
            }
            Ok(false) => {
                eprintln!("clamd antwortet, aber nicht mit PONG");
                ExitCode::FAILURE
            }
            Err(e) => {
                eprintln!("clamd nicht erreichbar: {e}");
                ExitCode::FAILURE
            }
        },
        Some("version") => match client.version() {
            Ok(v) => {
                println!("{v}");
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("Fehler: {e}");
                ExitCode::FAILURE
            }
        },
        Some("scan") => {
            let Some(path) = args.get(1) else {
                eprintln!("Verwendung: avox-service scan <PFAD>");
                return ExitCode::FAILURE;
            };
            match client.scan_path(PathBuf::from(path).as_path()) {
                Ok(report) => {
                    println!(
                        "Geprüft: {} · Funde: {} · Fehler: {}",
                        report.scanned,
                        report.findings.len(),
                        report.errors.len()
                    );
                    for f in &report.findings {
                        println!("  BEDROHUNG: {} — {}", f.path.display(), f.signature);
                    }
                    if report.is_infected() {
                        ExitCode::FAILURE
                    } else {
                        ExitCode::SUCCESS
                    }
                }
                Err(e) => {
                    eprintln!("Scan fehlgeschlagen: {e}");
                    ExitCode::FAILURE
                }
            }
        }
        _ => {
            eprintln!("Avox Service (Skelett)\n");
            eprintln!("Befehle: ping | version | scan <PFAD>");
            eprintln!("Umgebung: AVOX_CLAMD_ADDR (Default: 127.0.0.1:3310)");
            ExitCode::FAILURE
        }
    }
}
