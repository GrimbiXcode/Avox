//! Transport & Framing für die Avox-IPC.
//!
//! Ein [`Endpoint`] beschreibt, *wo* Service und GUI sich treffen. Auf Unix ist das
//! ein Domain-Socket (mit Dateisystem-Rechten als Zugriffsschutz), sonst ein
//! loopback-TCP-Port. [`read_msg`]/[`write_msg`] übernehmen das line-delimited-JSON-Framing.

use std::io::{self, BufRead, Write};
use std::net::{TcpListener, TcpStream, ToSocketAddrs};
use std::path::PathBuf;
use std::time::Duration;

use serde::de::DeserializeOwned;
use serde::Serialize;

/// Timeout für den Verbindungsaufbau eines Clients (schnelles Erkennen eines
/// nicht laufenden Dienstes).
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// Timeout fürs Senden einer Anfrage. Das **Lesen** bleibt bewusst unbegrenzt,
/// damit lange Scans (deren Antwort erst nach Abschluss kommt) nicht abbrechen.
const WRITE_TIMEOUT: Duration = Duration::from_secs(30);

/// Alles, worüber wir lesen und schreiben können; `Send`, damit Verbindungen in
/// Worker-Threads übergeben werden können.
pub trait Stream: io::Read + io::Write + Send {}
impl<T: io::Read + io::Write + Send> Stream for T {}

/// Adresse, unter der der Avox-Service lauscht.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Endpoint {
    /// Unix-Domain-Socket (Linux/macOS).
    Unix(PathBuf),
    /// Loopback-TCP `host:port`.
    Tcp(String),
}

impl Endpoint {
    /// Plattform-Default: Unix-Socket unter `/tmp`, sonst loopback-TCP.
    pub fn default_local() -> Self {
        #[cfg(unix)]
        {
            Endpoint::Unix(PathBuf::from("/tmp/avox-service.sock"))
        }
        #[cfg(not(unix))]
        {
            Endpoint::Tcp("127.0.0.1:7777".to_string())
        }
    }

    /// Interpretiert einen String: enthält er `:` und beginnt nicht mit `/`,
    /// gilt er als TCP-Endpunkt, sonst als Unix-Socket-Pfad.
    pub fn parse(s: &str) -> Self {
        if s.contains(':') && !s.starts_with('/') {
            Endpoint::Tcp(s.to_string())
        } else {
            Endpoint::Unix(PathBuf::from(s))
        }
    }
}

/// Lauschender Socket für den Service.
pub enum Listener {
    #[cfg(unix)]
    Unix {
        listener: std::os::unix::net::UnixListener,
        path: PathBuf,
    },
    Tcp(TcpListener),
}

impl Listener {
    /// Bindet den Endpoint. Ein bereits vorhandener Unix-Socket-Pfad wird entfernt
    /// (typisch nach unsauberem Beenden), damit der Bind gelingt.
    pub fn bind(endpoint: &Endpoint) -> io::Result<Self> {
        match endpoint {
            #[cfg(unix)]
            Endpoint::Unix(path) => {
                if path.exists() {
                    let _ = std::fs::remove_file(path);
                }
                let listener = std::os::unix::net::UnixListener::bind(path)?;
                // Härtung: nur der Eigentümer darf sich verbinden (rw-------).
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
                Ok(Listener::Unix {
                    listener,
                    path: path.clone(),
                })
            }
            #[cfg(not(unix))]
            Endpoint::Unix(_) => Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "Unix-Sockets werden auf dieser Plattform nicht unterstützt; TCP verwenden",
            )),
            Endpoint::Tcp(addr) => Ok(Listener::Tcp(TcpListener::bind(addr)?)),
        }
    }

    /// Nimmt eine eingehende Verbindung an.
    pub fn accept(&self) -> io::Result<Box<dyn Stream>> {
        match self {
            #[cfg(unix)]
            Listener::Unix { listener, .. } => Ok(Box::new(listener.accept()?.0)),
            Listener::Tcp(l) => Ok(Box::new(l.accept()?.0)),
        }
    }

    /// Tatsächlich gebundener Endpoint (bei TCP z. B. mit dem realen Port,
    /// wenn mit `:0` ein ephemerer Port angefordert wurde).
    pub fn local_addr(&self) -> io::Result<Endpoint> {
        match self {
            #[cfg(unix)]
            Listener::Unix { path, .. } => Ok(Endpoint::Unix(path.clone())),
            Listener::Tcp(l) => Ok(Endpoint::Tcp(l.local_addr()?.to_string())),
        }
    }
}

impl Drop for Listener {
    fn drop(&mut self) {
        // Räumt die Unix-Socket-Datei bei **geordnetem** Beenden auf (normaler Return,
        // Panic-Unwind). Achtung: bei Signal-Terminierung (SIGTERM/SIGKILL) läuft Drop
        // nicht — ein sauberer Socket-Cleanup dort erfordert einen Signal-Handler
        // (spätere Stufe). Unkritisch, da `bind()` einen vorhandenen Socket beim
        // Start ohnehin entfernt.
        #[cfg(unix)]
        if let Listener::Unix { path, .. } = self {
            let _ = std::fs::remove_file(path);
        }
    }
}

/// Verbindet sich als Client mit einem Endpoint. Setzt Connect-/Write-Timeouts;
/// der Lese-Timeout bleibt offen (lange Scans).
pub fn connect(endpoint: &Endpoint) -> io::Result<Box<dyn Stream>> {
    match endpoint {
        #[cfg(unix)]
        Endpoint::Unix(path) => {
            // Unix-Domain-Connect ist lokal praktisch sofort (oder scheitert sofort).
            let stream = std::os::unix::net::UnixStream::connect(path)?;
            stream.set_write_timeout(Some(WRITE_TIMEOUT))?;
            Ok(Box::new(stream))
        }
        #[cfg(not(unix))]
        Endpoint::Unix(_) => Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Unix-Sockets werden auf dieser Plattform nicht unterstützt; TCP verwenden",
        )),
        Endpoint::Tcp(addr) => {
            let sockaddr = addr.to_socket_addrs()?.next().ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("ungültige Adresse: {addr}"),
                )
            })?;
            let stream = TcpStream::connect_timeout(&sockaddr, CONNECT_TIMEOUT)?;
            stream.set_write_timeout(Some(WRITE_TIMEOUT))?;
            Ok(Box::new(stream))
        }
    }
}

/// Schreibt eine Nachricht als eine JSON-Zeile (`{…}\n`).
pub fn write_msg<W: Write, T: Serialize>(w: &mut W, msg: &T) -> io::Result<()> {
    let mut line = serde_json::to_vec(msg)?;
    line.push(b'\n');
    w.write_all(&line)?;
    w.flush()
}

/// Liest die nächste JSON-Zeile und deserialisiert sie.
///
/// Gibt `Ok(None)` zurück, wenn die Gegenseite die Verbindung sauber geschlossen hat.
pub fn read_msg<R: BufRead, T: DeserializeOwned>(r: &mut R) -> io::Result<Option<T>> {
    let mut line = String::new();
    let n = r.read_line(&mut line)?;
    if n == 0 {
        return Ok(None); // EOF
    }
    let trimmed = line.trim_end();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let msg = serde_json::from_str(trimmed)?;
    Ok(Some(msg))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_parse_distinguishes_tcp_and_unix() {
        assert_eq!(
            Endpoint::parse("127.0.0.1:3310"),
            Endpoint::Tcp("127.0.0.1:3310".into())
        );
        assert_eq!(
            Endpoint::parse("/tmp/x.sock"),
            Endpoint::Unix(PathBuf::from("/tmp/x.sock"))
        );
    }

    #[test]
    fn framing_roundtrip_over_buffer() {
        let mut buf: Vec<u8> = Vec::new();
        write_msg(&mut buf, &vec![1u32, 2, 3]).unwrap();
        write_msg(&mut buf, &"hallo".to_string()).unwrap();

        let mut reader = io::BufReader::new(&buf[..]);
        let a: Option<Vec<u32>> = read_msg(&mut reader).unwrap();
        let b: Option<String> = read_msg(&mut reader).unwrap();
        let c: Option<String> = read_msg(&mut reader).unwrap();
        assert_eq!(a, Some(vec![1, 2, 3]));
        assert_eq!(b, Some("hallo".to_string()));
        assert_eq!(c, None); // EOF
    }

    /// Voller Request/Response-Umlauf über einen **echten** Socket (loopback-TCP mit
    /// ephemerem Port, damit der Test auch unter Windows-CI läuft). Deckt bind →
    /// local_addr → accept → connect → Framing in beide Richtungen ab.
    #[test]
    fn request_response_over_real_socket() {
        use crate::{Request, RequestEnvelope, Response, ResponseEnvelope};
        use std::io::BufReader;
        use std::thread;

        let listener = Listener::bind(&Endpoint::Tcp("127.0.0.1:0".into())).unwrap();
        let endpoint = listener.local_addr().unwrap();

        // Server: eine Verbindung annehmen, eine Anfrage beantworten.
        let server = thread::spawn(move || {
            let conn = listener.accept().unwrap();
            let mut reader = BufReader::new(conn);
            let env: RequestEnvelope = read_msg(&mut reader).unwrap().unwrap();
            let response = match env.request {
                Request::Ping => Response::Pong,
                _ => Response::Error("unerwartet".into()),
            };
            write_msg(
                reader.get_mut(),
                &ResponseEnvelope {
                    id: env.id,
                    response,
                },
            )
            .unwrap();
        });

        // Client: verbinden, Ping senden, Pong erwarten.
        let conn = connect(&endpoint).unwrap();
        let mut reader = BufReader::new(conn);
        write_msg(
            reader.get_mut(),
            &RequestEnvelope {
                id: 7,
                request: Request::Ping,
            },
        )
        .unwrap();
        let resp: ResponseEnvelope = read_msg(&mut reader).unwrap().unwrap();

        assert_eq!(resp.id, 7);
        assert_eq!(resp.response, Response::Pong);
        server.join().unwrap();
    }
}
