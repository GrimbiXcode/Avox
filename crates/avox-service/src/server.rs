//! IPC-Server: nimmt Verbindungen an, verarbeitet Anfragen und delegiert an
//! Engine (clamd), freshclam und Quarantäne.
//!
//! Ein Thread pro Verbindung, sequentielles Request/Response-Protokoll je Verbindung.

use std::io::BufReader;
use std::process::Command;
use std::thread;

use avox_core::ThreatAction;
use avox_engine::ClamdClient;
use avox_ipc::transport::{self, Listener};
use avox_ipc::{Request, RequestEnvelope, Response, ResponseEnvelope};

use crate::config::Config;
use crate::quarantine::{self, Quarantine};
use crate::scan::{self, ScanTarget};
use crate::scheduler;

/// Startet den Server und lauscht dauerhaft auf dem konfigurierten Endpoint.
pub fn run(config: Config) -> std::io::Result<()> {
    let listener = Listener::bind(&config.ipc)?;
    scheduler::start(config.clone());
    eprintln!("avox-service lauscht auf {:?}", config.ipc);
    loop {
        match listener.accept() {
            Ok(conn) => {
                let cfg = config.clone();
                thread::spawn(move || {
                    if let Err(e) = handle_connection(conn, cfg) {
                        eprintln!("Verbindungsfehler: {e}");
                    }
                });
            }
            Err(e) => eprintln!("accept fehlgeschlagen: {e}"),
        }
    }
}

/// Verarbeitet eine Verbindung: liest Anfragen, schreibt Antworten (gleiche `id`).
fn handle_connection(conn: Box<dyn transport::Stream>, cfg: Config) -> std::io::Result<()> {
    let mut reader = BufReader::new(conn);
    while let Some(env) = transport::read_msg::<_, RequestEnvelope>(&mut reader)? {
        eprintln!("[ipc] #{} {}", env.id, request_label(&env.request));
        let response = dispatch(&cfg, env.request);
        let out = ResponseEnvelope {
            id: env.id,
            response,
        };
        // BufReader puffert nur Lesevorgänge; Schreiben geht direkt an den Stream.
        transport::write_msg(reader.get_mut(), &out)?;
    }
    Ok(())
}

/// Kurzes Log-Label für eine Anfrage (ohne sensible Details aufzublähen).
fn request_label(request: &Request) -> String {
    match request {
        Request::Ping => "Ping".to_string(),
        Request::GetVersion => "GetVersion".to_string(),
        Request::Scan { path } => format!("Scan {}", path.display()),
        Request::FullScan => "FullScan".to_string(),
        Request::GetSchedule => "GetSchedule".to_string(),
        Request::ApplyAction { path, action } => {
            format!("ApplyAction {action:?} {}", path.display())
        }
        Request::ListQuarantine => "ListQuarantine".to_string(),
        Request::RestoreQuarantine { id } => format!("RestoreQuarantine {id}"),
        Request::UpdateSignatures => "UpdateSignatures".to_string(),
    }
}

/// Bildet eine Anfrage auf eine Antwort ab. Fehler werden zu [`Response::Error`],
/// damit eine Verbindung nicht an einer einzelnen fehlerhaften Anfrage stirbt.
pub fn dispatch(cfg: &Config, request: Request) -> Response {
    let client = ClamdClient::new(cfg.clamd.clone());
    match request {
        Request::Ping => Response::Pong,

        Request::GetVersion => match client.version() {
            Ok(clamd) => Response::Version {
                service: env!("CARGO_PKG_VERSION").to_string(),
                clamd,
            },
            Err(e) => Response::Error(format!("clamd-Version nicht abrufbar: {e}")),
        },

        Request::Scan { path } => match client.scan_path(&path) {
            Ok(report) => Response::ScanResult(report),
            Err(e) => Response::Error(format!("Scan fehlgeschlagen: {e}")),
        },

        Request::FullScan => match scan::run(cfg, &ScanTarget::Full) {
            Ok(report) => Response::ScanResult(report),
            Err(e) => Response::Error(format!("Vollscan fehlgeschlagen: {e}")),
        },

        Request::GetSchedule => Response::Schedule(cfg.schedule_infos()),

        Request::UpdateSignatures => update_signatures(cfg),

        Request::ApplyAction { path, action } => apply_action(cfg, &path, action),

        Request::ListQuarantine => {
            match Quarantine::new(&cfg.quarantine_dir).and_then(|q| q.list()) {
                Ok(list) => Response::QuarantineList(list),
                Err(e) => Response::Error(format!("Quarantäne-Liste fehlgeschlagen: {e}")),
            }
        }

        Request::RestoreQuarantine { id } => {
            match Quarantine::new(&cfg.quarantine_dir).and_then(|q| q.restore(&id)) {
                Ok(entry) => Response::ActionApplied {
                    detail: format!("wiederhergestellt: {}", entry.original_path.display()),
                },
                Err(e) => Response::Error(format!("Wiederherstellung fehlgeschlagen: {e}")),
            }
        }
    }
}

/// Führt freshclam aus und fasst das Ergebnis zusammen.
fn update_signatures(cfg: &Config) -> Response {
    let mut cmd = Command::new(&cfg.freshclam_bin);
    if let Some(conf) = &cfg.freshclam_conf {
        cmd.arg(format!("--config-file={}", conf.display()));
    }
    match cmd.output() {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            // Letzte aussagekräftige Zeile als Zusammenfassung.
            let summary = stdout
                .lines()
                .rev()
                .find(|l| !l.trim().is_empty())
                .unwrap_or("freshclam ausgeführt")
                .trim()
                .to_string();
            if out.status.success() {
                Response::SignaturesUpdated { summary }
            } else {
                Response::Error(format!(
                    "freshclam-Exitcode {:?}: {summary}",
                    out.status.code()
                ))
            }
        }
        Err(e) => Response::Error(format!(
            "freshclam nicht ausführbar ({}): {e}",
            cfg.freshclam_bin
        )),
    }
}

/// Wendet eine Aktion auf einen Fund an.
fn apply_action(cfg: &Config, path: &std::path::Path, action: ThreatAction) -> Response {
    match action {
        ThreatAction::Quarantine => {
            match Quarantine::new(&cfg.quarantine_dir).and_then(|q| q.quarantine(path)) {
                Ok(entry) => Response::ActionApplied {
                    detail: format!("in Quarantäne verschoben als {}", entry.id),
                },
                Err(e) => Response::Error(format!("Quarantäne fehlgeschlagen: {e}")),
            }
        }
        ThreatAction::Delete => match quarantine::delete(path) {
            Ok(()) => Response::ActionApplied {
                detail: format!("gelöscht: {}", path.display()),
            },
            Err(e) => Response::Error(format!("Löschen fehlgeschlagen: {e}")),
        },
        ThreatAction::Ignore => Response::ActionApplied {
            detail: "ignoriert (persistente Whitelist folgt)".to_string(),
        },
    }
}
