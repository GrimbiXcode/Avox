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
    ///
    /// clamd meldet bei `CONTSCAN` nur befallene Dateien und Fehler zurück — für
    /// saubere Dateien kommt nichts. Die Gesamtzahl geprüfter Dateien ermitteln wir
    /// daher selbst per Dateisystem-Walk (Konvention wie `clamdscan`). Dieser Wert
    /// kann von clamds interner Zählung abweichen (z. B. Dateien in Archiven).
    pub fn scan_path(&self, path: &Path) -> io::Result<ScanReport> {
        let path_str = path.to_string_lossy();
        let raw = self.command(&format!("CONTSCAN {path_str}"))?;
        let mut report = parse_scan_response(&raw);
        report.scanned = count_files(path);
        Ok(report)
    }

    /// Sendet ein clamd-Kommando im `z`-Format und liest die vollständige Antwort.
    fn command(&self, cmd: &str) -> io::Result<String> {
        let mut stream = self.connect()?;
        // 'z'-Präfix: Kommando mit NUL terminiert.
        stream.write_all(format!("z{cmd}\0").as_bytes())?;
        stream.flush()?;
        let mut buf = Vec::new();
        stream.read_to_end(&mut buf)?;
        // 'z'-Antworten sind NUL-terminiert; das abschließende NUL entfernen.
        // Interne NUL-Trenner (mehrzeilige Scan-Antwort) bleiben erhalten.
        let s = String::from_utf8_lossy(&buf).into_owned();
        Ok(s.trim_end_matches('\0').to_string())
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

/// Zählt reguläre Dateien unter `path` (rekursiv). Symlinks werden nicht verfolgt,
/// um Zyklen zu vermeiden. Eine einzelne Datei zählt als 1, unlesbare Pfade als 0.
fn count_files(path: &Path) -> u64 {
    let Ok(meta) = std::fs::symlink_metadata(path) else {
        return 0;
    };
    if meta.is_file() {
        return 1;
    }
    if !meta.is_dir() {
        return 0;
    }
    let Ok(entries) = std::fs::read_dir(path) else {
        return 0;
    };
    let mut count = 0;
    for entry in entries.flatten() {
        let child = entry.path();
        let Ok(m) = std::fs::symlink_metadata(&child) else {
            continue;
        };
        if m.file_type().is_symlink() {
            continue; // Symlinks nicht folgen
        } else if m.is_dir() {
            count += count_files(&child);
        } else if m.is_file() {
            count += 1;
        }
    }
    count
}

/// Parst die (mehrzeilige, `\0`- oder `\n`-getrennte) clamd-Scan-Antwort.
/// Setzt nur `findings` und `errors`; die Gesamtzahl (`scanned`) wird separat
/// über [`count_files`] bestimmt, da clamd saubere Dateien nicht meldet.
fn parse_scan_response(raw: &str) -> ScanReport {
    let mut report = ScanReport::default();
    for line in raw.split(['\0', '\n']) {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
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
        assert_eq!(r.findings.len(), 1);
        assert_eq!(r.findings[0].signature, "Eicar-Test-Signature");
    }

    #[test]
    fn parses_error_line_and_ignores_ok() {
        // clamd meldet keine OK-Zeilen; falls doch vorhanden, dürfen sie nichts auslösen.
        let raw = "/tmp/a: OK\n/tmp/b: Can't open file ERROR\n";
        let r = parse_scan_response(raw);
        assert!(!r.is_infected());
        assert_eq!(r.errors.len(), 1);
        assert!(r.findings.is_empty());
    }

    #[test]
    fn count_files_counts_regular_files_recursively() {
        let base = std::env::temp_dir().join(format!("avox-count-{}", std::process::id()));
        let sub = base.join("sub");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(base.join("a.txt"), b"a").unwrap();
        std::fs::write(base.join("b.txt"), b"b").unwrap();
        std::fs::write(sub.join("c.txt"), b"c").unwrap();

        assert_eq!(count_files(&base), 3);
        assert_eq!(count_files(&base.join("a.txt")), 1);
        assert_eq!(count_files(&base.join("does-not-exist")), 0);

        std::fs::remove_dir_all(&base).ok();
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
