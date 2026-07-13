# Avox

**Benutzerfreundlicher, quelloffener Antivirus auf Basis von [ClamAV](https://www.clamav.net/)**
mit grafischer Oberfläche für **Windows, macOS und Linux** (amd64, arm64, 32-Bit).

> Status: **Frühe Entwicklung (Skelett / M0–M1).** Noch nicht für den produktiven
> Einsatz gedacht. Siehe [`PLAN.md`](./PLAN.md) für Vision, Architektur und Roadmap.

## Architektur (Kurzfassung)

Avox spricht den **`clamd`-Daemon über IPC** an (statt `libclamav` zu linken) und
trennt einen **privilegierten Dienst** von einer **unprivilegierten GUI** (geplant: Tauri).

```
GUI (Tauri)  ──IPC──►  avox-service  ──►  clamd / freshclam
```

Details: [`PLAN.md`](./PLAN.md).

## Workspace

| Crate | Zweck |
|---|---|
| `avox-core` | Geteilte Domänentypen (Scan-Ergebnis, Aktionen, Konfiguration) |
| `avox-ipc` | Nachrichten-Vertrag **und** Transport (Unix-Socket/TCP, JSON-Framing) |
| `avox-engine` | clamd-IPC-Client (Ping, Version, Scan) |
| `avox-service` | Privilegierter Dienst: IPC-Server, Quarantäne, freshclam |
| `app/` | Tauri-GUI — folgt in M3 |

## Schnellstart (Entwicklung)

Voraussetzung: Rust (stable) und ein laufender `clamd`.

```bash
# Bauen & Tests
cargo build --workspace
cargo test  --workspace

# Direkt gegen clamd (Diagnose, TCP 127.0.0.1:3310 als Default)
cargo run -p avox-service -- ping
cargo run -p avox-service -- version
cargo run -p avox-service -- scan ./pfad/zum/ordner

# clamd-Adresse überschreiben (TCP oder Unix-Socket-Pfad)
AVOX_CLAMD_ADDR=/var/run/clamav/clamd.ctl cargo run -p avox-service -- ping
```

### IPC-Server & -Client (M2)

Der Service kann als Daemon lauschen; ein Client (später die GUI) spricht ihn über
einen Unix-Socket (bzw. loopback-TCP) an:

```bash
# Terminal 1: Server starten (Default-Socket /tmp/avox-service.sock)
cargo run -p avox-service -- serve

# Terminal 2: Anfragen als Client
cargo run -p avox-service -- call ping
cargo run -p avox-service -- call version
cargo run -p avox-service -- call scan ./pfad/zum/ordner
cargo run -p avox-service -- call quarantine ./verdaechtige-datei
cargo run -p avox-service -- call list              # Quarantäne auflisten
cargo run -p avox-service -- call restore <ID>      # Datei zurückstellen
cargo run -p avox-service -- call update            # freshclam

# Endpoint & Pfade überschreiben
AVOX_IPC=127.0.0.1:7777 cargo run -p avox-service -- serve
AVOX_QUARANTINE_DIR=~/.avox/quarantine  AVOX_FRESHCLAM_CONF=/pfad/freshclam.conf ...
```

Integrationstest gegen laufenden clamd:

```bash
cargo test -p avox-engine -- --ignored
```

Lokales clamd-Setup: siehe [`docs/dev-setup.md`](./docs/dev-setup.md).

## Mitmachen

Siehe [`CONTRIBUTING.md`](./CONTRIBUTING.md) und [`SECURITY.md`](./SECURITY.md).

## Lizenz

[GPL-2.0-only](./LICENSE). ClamAV steht ebenfalls unter GPLv2.
