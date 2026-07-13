//! `avox-engine` — Anbindung an den ClamAV-Daemon (`clamd`) über IPC.
//!
//! Bewusste Architekturentscheidung: Avox **linkt `libclamav` nicht**, sondern
//! spricht den laufenden `clamd`-Prozess über einen Socket an. Das vermeidet die
//! GPL-Ableitung, trennt die Prozesse sauber und vereinfacht das Deployment.
//!
//! Implementiert ist eine minimale, aber echte Teilmenge des clamd-Protokolls
//! (`PING`, `VERSION`, `CONTSCAN`). Kommandos werden im `z`-Format gesendet
//! (Präfix `z`, mit `\0` terminiert), Antworten werden bis EOF gelesen.

use std::io::{self, Read, Write};
use std::net::TcpStream;
use std::path::Path;
use std::time::Duration;

use avox_core::{ClamdAddress, Finding, ScanReport};

/// Verbindungs-Timeout für clamd-Kommandos.
const IO_TIMEOUT: Duration = Duration::from_secs(30);

/// Client für einen `clamd`-Daemon.
#[derive(Debug, Clone)]
pub struct ClamdClient {
    addr: ClamdAddress,
}

impl ClamdClient {
    /// Erzeugt einen Client für die gegebene clamd-Adresse.
    pub fn new(addr: ClamdAddress) -> Self {
        Self { addr }
    }

    /// Verbindungstest. Gibt `true` zurück, wenn clamd mit `PONG` antwortet.
    pub fn ping(&self) -> io::Result<bool> {
        Ok(self.command("PING")?.trim() == "PONG")
    }

    /// Liefert die von clamd gemeldete Versionszeile.
    pub fn version(&self) -> io::Result<String> {
        Ok(self.command("VERSION")?.trim().to_string())
    }

    /// Scannt einen Pfad (Datei oder Ordner, rekursiv via `CONTSCAN`).
    pub fn scan_path(&self, path: &Path) -> io::Result<ScanReport> {
        let path_str = path.to_string_lossy();
        let raw = self.command(&format!("CONTSCAN {path_str}"))?;
        Ok(parse_scan_response(&raw))
    }

    /// Sendet ein clamd-Kommando im `z`-Format und liest die vollständige Antwort.
    fn command(&self, cmd: &str) -> io::Result<String> {
        let mut stream = self.connect()?;
        // 'z'-Präfix: Kommando mit NUL terminiert.
        stream.write_all(format!("z{cmd}\0").as_bytes())?;
        stream.flush()?;
        let mut buf = Vec::new();
        stream.read_to_end(&mut buf)?;
        Ok(String::from_utf8_lossy(&buf).into_owned())
    }

    /// Öffnet die Verbindung passend zum Adresstyp.
    fn connect(&self) -> io::Result<Box<dyn ReadWrite>> {
        match &self.addr {
            ClamdAddress::Tcp(hostport) => {
                let stream = TcpStream::connect(hostport)?;
                stream.set_read_timeout(Some(IO_TIMEOUT))?;
                stream.set_write_timeout(Some(IO_TIMEOUT))?;
                Ok(Box::new(stream))
            }
            #[cfg(unix)]
            ClamdAddress::Unix(path) => {
                use std::os::unix::net::UnixStream;
                let stream = UnixStream::connect(path)?;
                stream.set_read_timeout(Some(IO_TIMEOUT))?;
                stream.set_write_timeout(Some(IO_TIMEOUT))?;
                Ok(Box::new(stream))
            }
            #[cfg(not(unix))]
            ClamdAddress::Unix(_) => Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "Unix-Domain-Sockets werden auf dieser Plattform nicht unterstützt; TCP verwenden",
            )),
        }
    }
}

/// Marker-Trait, damit sowohl `TcpStream` als auch `UnixStream` als Verbindung dienen.
trait ReadWrite: Read + Write {}
impl<T: Read + Write> ReadWrite for T {}

/// Parst die (mehrzeilige, `\0`- oder `\n`-getrennte) clamd-Scan-Antwort.
fn parse_scan_response(raw: &str) -> ScanReport {
    let mut report = ScanReport::default();
    for line in raw.split(['\0', '\n']) {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        report.scanned += 1;
        if let Some(rest) = line.strip_suffix(" FOUND") {
            // Format: "<pfad>: <signatur> FOUND"
            if let Some((path, sig)) = rest.rsplit_once(": ") {
                report.findings.push(Finding {
                    path: path.into(),
                    signature: sig.trim().to_string(),
                });
            }
        } else if let Some(rest) = line.strip_suffix(" ERROR") {
            if let Some((path, err)) = rest.rsplit_once(": ") {
                report.errors.push((path.into(), err.trim().to_string()));
            } else {
                report.errors.push(("".into(), rest.trim().to_string()));
            }
        }
        // "<pfad>: OK" → sauber, keine weitere Aktion nötig.
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_found_line() {
        let raw = "/tmp/eicar.txt: Eicar-Test-Signature FOUND\0";
        let r = parse_scan_response(raw);
        assert_eq!(r.scanned, 1);
        assert_eq!(r.findings.len(), 1);
        assert_eq!(r.findings[0].signature, "Eicar-Test-Signature");
    }

    #[test]
    fn parses_clean_and_error_lines() {
        let raw = "/tmp/a: OK\n/tmp/b: Can't open file ERROR\n";
        let r = parse_scan_response(raw);
        assert_eq!(r.scanned, 2);
        assert!(!r.is_infected());
        assert_eq!(r.errors.len(), 1);
    }

    /// Integrationstest gegen einen laufenden clamd. Standardmäßig ignoriert.
    /// Ausführen mit: `cargo test -p avox-engine -- --ignored`
    #[test]
    #[ignore]
    fn live_ping() {
        let client = ClamdClient::new(ClamdAddress::default());
        assert!(client.ping().unwrap());
    }
}
