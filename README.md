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
| `avox-ipc` | Nachrichten-Vertrag zwischen Service und GUI |
| `avox-engine` | clamd-IPC-Client (Ping, Version, Scan) |
| `avox-service` | Privilegierter Hintergrunddienst (Skelett-CLI) |
| `app/` | Tauri-GUI — folgt in M3 |

## Schnellstart (Entwicklung)

Voraussetzung: Rust (stable) und ein laufender `clamd`.

```bash
# Bauen & Tests
cargo build --workspace
cargo test  --workspace

# Gegen einen laufenden clamd (TCP 127.0.0.1:3310 als Default)
cargo run -p avox-service -- ping
cargo run -p avox-service -- version
cargo run -p avox-service -- scan ./pfad/zum/ordner

# clamd-Adresse überschreiben (TCP oder Unix-Socket-Pfad)
AVOX_CLAMD_ADDR=/var/run/clamav/clamd.ctl cargo run -p avox-service -- ping
```

Integrationstest gegen laufenden clamd:

```bash
cargo test -p avox-engine -- --ignored
```

## Mitmachen

Siehe [`CONTRIBUTING.md`](./CONTRIBUTING.md) und [`SECURITY.md`](./SECURITY.md).

## Lizenz

[GPL-2.0-only](./LICENSE). ClamAV steht ebenfalls unter GPLv2.
