//! Transport & Framing für die Avox-IPC.
//!
//! Ein [`Endpoint`] beschreibt, *wo* Service und GUI sich treffen. Auf Unix ist das
//! ein Domain-Socket (mit Dateisystem-Rechten als Zugriffsschutz), sonst ein
//! loopback-TCP-Port. [`read_msg`]/[`write_msg`] übernehmen das line-delimited-JSON-Framing.

use std::io::{self, BufRead, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;

use serde::de::DeserializeOwned;
use serde::Serialize;

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
    Unix(std::os::unix::net::UnixListener),
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
                Ok(Listener::Unix(listener))
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
            Listener::Unix(l) => Ok(Box::new(l.accept()?.0)),
            Listener::Tcp(l) => Ok(Box::new(l.accept()?.0)),
        }
    }
}

/// Verbindet sich als Client mit einem Endpoint.
pub fn connect(endpoint: &Endpoint) -> io::Result<Box<dyn Stream>> {
    match endpoint {
        #[cfg(unix)]
        Endpoint::Unix(path) => Ok(Box::new(std::os::unix::net::UnixStream::connect(path)?)),
        #[cfg(not(unix))]
        Endpoint::Unix(_) => Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Unix-Sockets werden auf dieser Plattform nicht unterstützt; TCP verwenden",
        )),
        Endpoint::Tcp(addr) => Ok(Box::new(TcpStream::connect(addr)?)),
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
}
