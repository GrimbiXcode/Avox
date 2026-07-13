//! `avox-service` — privilegierter Hintergrunddienst von Avox.
//!
//! Betriebsarten:
//! - `serve`                — IPC-Server starten (GUI/Client verbinden sich hierauf)
//! - `call <cmd>`           — als Client eine Anfrage an einen laufenden Server senden
//! - `ping|version|scan`    — Direkt-Kommandos an clamd (ohne IPC, für Diagnose)
//!
//! Wichtige Umgebungsvariablen (siehe `config.rs`):
//!   AVOX_CLAMD_ADDR (Default 127.0.0.1:3310), AVOX_IPC, AVOX_QUARANTINE_DIR,
//!   AVOX_FRESHCLAM, AVOX_FRESHCLAM_CONF

mod config;
mod quarantine;
mod server;

use std::io::{self, BufReader};
use std::path::PathBuf;
use std::process::ExitCode;

use avox_engine::ClamdClient;
use avox_ipc::transport::{self, Endpoint};
use avox_ipc::{Request, RequestEnvelope, Response, ResponseEnvelope};

use crate::config::Config;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cfg = Config::from_env();

    match args.first().map(String::as_str) {
        Some("serve") => match server::run(cfg) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("Server-Fehler: {e}");
                ExitCode::FAILURE
            }
        },
        Some("call") => cmd_call(&cfg.ipc, &args[1..]),
        Some("ping") => direct_ping(&cfg),
        Some("version") => direct_version(&cfg),
        Some("scan") => direct_scan(&cfg, args.get(1)),
        _ => {
            usage();
            ExitCode::FAILURE
        }
    }
}

fn usage() {
    eprintln!("Avox Service\n");
    eprintln!("  serve                          IPC-Server starten");
    eprintln!("  call ping|version|update       Anfrage an laufenden Server");
    eprintln!("  call scan <PFAD>");
    eprintln!("  call quarantine|delete <PFAD>");
    eprintln!("  ping|version|scan <PFAD>       Direkt an clamd (Diagnose)");
    eprintln!("\nUmgebung: AVOX_CLAMD_ADDR, AVOX_IPC, AVOX_QUARANTINE_DIR, AVOX_FRESHCLAM[_CONF]");
}

/// IPC-Client: eine Anfrage senden, eine Antwort empfangen.
fn call(endpoint: &Endpoint, request: Request) -> io::Result<Response> {
    let conn = transport::connect(endpoint)?;
    let mut reader = BufReader::new(conn);
    transport::write_msg(reader.get_mut(), &RequestEnvelope { id: 1, request })?;
    match transport::read_msg::<_, ResponseEnvelope>(&mut reader)? {
        Some(env) => Ok(env.response),
        None => Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "keine Antwort vom Server",
        )),
    }
}

fn cmd_call(endpoint: &Endpoint, args: &[String]) -> ExitCode {
    let request = match args.first().map(String::as_str) {
        Some("ping") => Request::Ping,
        Some("version") => Request::GetVersion,
        Some("update") => Request::UpdateSignatures,
        Some("scan") => match args.get(1) {
            Some(p) => Request::Scan {
                path: PathBuf::from(p),
            },
            None => {
                eprintln!("Verwendung: call scan <PFAD>");
                return ExitCode::FAILURE;
            }
        },
        Some("quarantine") | Some("delete") => match args.get(1) {
            Some(p) => Request::ApplyAction {
                path: PathBuf::from(p),
                action: if args[0] == "delete" {
                    avox_core::ThreatAction::Delete
                } else {
                    avox_core::ThreatAction::Quarantine
                },
            },
            None => {
                eprintln!("Verwendung: call {} <PFAD>", args[0]);
                return ExitCode::FAILURE;
            }
        },
        _ => {
            usage();
            return ExitCode::FAILURE;
        }
    };

    match call(endpoint, request) {
        Ok(response) => print_response(&response),
        Err(e) => {
            eprintln!("IPC-Fehler: {e}");
            ExitCode::FAILURE
        }
    }
}

fn print_response(response: &Response) -> ExitCode {
    match response {
        Response::Pong => {
            println!("Pong");
            ExitCode::SUCCESS
        }
        Response::Version { service, clamd } => {
            println!("avox-service {service} · clamd: {clamd}");
            ExitCode::SUCCESS
        }
        Response::ScanResult(report) => {
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
        Response::ActionApplied { detail } => {
            println!("OK: {detail}");
            ExitCode::SUCCESS
        }
        Response::SignaturesUpdated { summary } => {
            println!("Signaturen: {summary}");
            ExitCode::SUCCESS
        }
        Response::Error(msg) => {
            eprintln!("Fehler: {msg}");
            ExitCode::FAILURE
        }
    }
}

// --- Direkt-Kommandos an clamd (ohne IPC), nützlich zur Diagnose ---

fn direct_ping(cfg: &Config) -> ExitCode {
    match ClamdClient::new(cfg.clamd.clone()).ping() {
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
    }
}

fn direct_version(cfg: &Config) -> ExitCode {
    match ClamdClient::new(cfg.clamd.clone()).version() {
        Ok(v) => {
            println!("{v}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("Fehler: {e}");
            ExitCode::FAILURE
        }
    }
}

fn direct_scan(cfg: &Config, path: Option<&String>) -> ExitCode {
    let Some(path) = path else {
        eprintln!("Verwendung: avox-service scan <PFAD>");
        return ExitCode::FAILURE;
    };
    match ClamdClient::new(cfg.clamd.clone()).scan_path(PathBuf::from(path).as_path()) {
        Ok(report) => print_response(&Response::ScanResult(report)),
        Err(e) => {
            eprintln!("Scan fehlgeschlagen: {e}");
            ExitCode::FAILURE
        }
    }
}
