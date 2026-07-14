# Avox

**Benutzerfreundlicher, quelloffener Antivirus auf Basis von [ClamAV](https://www.clamav.net/)**
mit grafischer Oberfläche für **Windows, macOS und Linux** (amd64, arm64, 32-Bit).

> Status: **v0.1.0** — Scan, Quarantäne (mit Wiederherstellung), Zeitpläne,
> Signatur-Updates und GUI mit Tray. Noch unsigniert/nicht notarisiert. Siehe
> [`PLAN.md`](./PLAN.md) für Vision, Architektur und Roadmap.

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
| `app/` | **Tauri-v2-GUI** (Dashboard, Scan, Quarantäne) — [Details](./app/README.md) |

## Voraussetzung: ClamAV (`clamd`)

Avox nutzt die ClamAV-Engine über den Daemon **`clamd`** — dieser muss laufen (mit
aktuellen Signaturen). Die GUI startet den Avox-Dienst selbst, aber **`clamd` musst
du einmalig installieren und starten.**

**macOS (Homebrew):**
```bash
brew install clamav
# Konfiguration anlegen (Beispiele aus den .sample-Dateien):
cp /opt/homebrew/etc/clamav/freshclam.conf.sample /opt/homebrew/etc/clamav/freshclam.conf
cp /opt/homebrew/etc/clamav/clamd.conf.sample     /opt/homebrew/etc/clamav/clamd.conf
# in beiden die Zeile "Example" auskommentieren/entfernen; in clamd.conf setzen:
#   TCPSocket 3310
#   TCPAddr 127.0.0.1
mkdir -p /opt/homebrew/var/run/clamav /opt/homebrew/var/log/clamav
freshclam                       # Signaturen laden
clamd                           # Daemon starten
```

**Linux (Debian/Ubuntu):**
```bash
sudo apt install clamav clamav-daemon clamav-freshclam
sudo systemctl enable --now clamav-freshclam clamav-daemon
```

**Windows:** ClamAV-Installer von [clamav.net/downloads](https://www.clamav.net/downloads)
laden, `clamd.conf` mit `TCPSocket 3310` / `TCPAddr 127.0.0.1` einrichten, `freshclam`
ausführen und `clamd` als Dienst starten.

Ausführliche, reproduzierbare Schritt-für-Schritt-Anleitung (macOS): siehe
[`docs/dev-setup.md`](./docs/dev-setup.md).

## Autostart / Hintergrunddienst

Die GUI richtet den Autostart **selbst** ein — plattformübliches Pendant je OS:

| Plattform | Mechanismus | avox-service | clamd / freshclam |
|---|---|---|---|
| **macOS** | launchd (`~/Library/LaunchAgents`) | von der App | von der App (Homebrew hat keinen Dienst) |
| **Linux** | systemd-User-Unit (`~/.config/systemd/user`) | von der App | **Distributions-Dienst** (`clamav-daemon`, `clamav-freshclam`) |
| **Windows** | geplante Aufgabe (`schtasks`, ONLOGON) | von der App | **ClamAV-Installer-Dienst** |

Der Avox-Dienst wird also überall automatisch beim Login gestartet. **clamd** verwaltet
die App nur dort selbst, wo die ClamAV-Distribution keinen eigenen Dienst mitbringt
(macOS/Homebrew); auf Linux/Windows übernehmen das die ClamAV-Pakete. Läuft `clamd`
nicht, zeigt die App einen Hinweis. Schlägt die Autostart-Einrichtung fehl, startet die
App den Dienst als Fallback direkt.

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
cargo run -p avox-service -- call full-scan         # Vollscan der konfigurierten Pfade
cargo run -p avox-service -- call schedule          # Zeitpläne anzeigen
cargo run -p avox-service -- call list              # Quarantäne auflisten
cargo run -p avox-service -- call restore <ID>      # Datei zurückstellen
cargo run -p avox-service -- call update            # freshclam

# Endpoint & Pfade überschreiben
AVOX_IPC=127.0.0.1:7777 cargo run -p avox-service -- serve
AVOX_QUARANTINE_DIR=~/.avox/quarantine  AVOX_FRESHCLAM_CONF=/pfad/freshclam.conf ...
```

### Zeitpläne & Vollscan (Config-Datei)

Zeitgesteuerte Scans und Vollscan-Pfade kommen aus einer JSON-Config
(`AVOX_CONFIG`, Default `~/.config/avox/config.json`). Der Dienst startet pro
Zeitplan einen Thread; Funde werden gemeldet und optional automatisch in
Quarantäne verschoben (`auto_quarantine`, Default: nur melden).

```json
{
  "schedules": [
    { "every_secs": 86400, "path": "/Users/ich/Downloads", "label": "Täglich Downloads" },
    { "every_secs": 604800, "full": true, "auto_quarantine": true, "label": "Wöchentlicher Vollscan" }
  ],
  "full_scan_paths": ["/Users/ich"]
}
```

Autostart des Dienstes (systemd/launchd) und das GUI-Tray:
siehe [`platform/README.md`](./platform/README.md).

Integrationstest gegen laufenden clamd:

```bash
cargo test -p avox-engine -- --ignored
```

Lokales clamd-Setup: siehe [`docs/dev-setup.md`](./docs/dev-setup.md).

## Installer / Pakete

Die Installer (`.dmg`, `.deb`/`.rpm`/`.AppImage`, `.msi`) baut Tauri aus `app/`:

```bash
cd app && npm ci && npx tauri build     # Bundles für die aktuelle Plattform
```

CI baut die Bundles plattformübergreifend ([`app.yml`](./.github/workflows/app.yml));
ein Tag `vX.Y.Z` erzeugt einen Release-Entwurf mit Installern
([`release.yml`](./.github/workflows/release.yml)). Details & Signierung:
[`packaging/README.md`](./packaging/README.md).

### Nach dem Download öffnen (wichtig!)

Die Bundles sind derzeit **nicht signiert/notarisiert**. Beim ersten Start blockiert
macOS (Gatekeeper) heruntergeladene, nicht-notarisierte Apps — die App scheint dann
„hängen zu bleiben" oder lässt sich nicht öffnen.

**macOS** – einmalig eine der beiden Varianten:
```bash
# Quarantäne-Markierung entfernen …
xattr -dr com.apple.quarantine /Applications/Avox.app
```
… oder im Finder **Rechtsklick auf Avox.app → „Öffnen"** und den Dialog bestätigen.

**Windows:** Beim SmartScreen-Dialog „Weitere Informationen" → „Trotzdem ausführen".

> Diese Reibung verschwindet, sobald das Projekt Developer-ID-Signatur +
> Notarisierung (macOS) bzw. EV-Signing (Windows) hat — siehe
> [`packaging/README.md`](./packaging/README.md).

## Mitmachen

Siehe [`CONTRIBUTING.md`](./CONTRIBUTING.md) und [`SECURITY.md`](./SECURITY.md).

## Lizenz

[GPL-2.0-only](./LICENSE). ClamAV steht ebenfalls unter GPLv2.
