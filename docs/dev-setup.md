# Entwickler-Setup: lokales clamd für Avox

Avox spricht `clamd` über IPC an (ADR-0001). Für Live-Scans muss lokal ein `clamd`
mit geladenen Signaturen laufen. Anleitung für **macOS (Homebrew)**; Linux analog.

## 1. ClamAV installieren
```bash
brew install clamav
```

## 2. Konfiguration
Minimal-Configs (siehe Beispiele in diesem Repo unter `docs/examples/`):

`/opt/homebrew/etc/clamav/freshclam.conf`
```
DatabaseDirectory /opt/homebrew/var/lib/clamav
DatabaseMirror database.clamav.net
UpdateLogFile /opt/homebrew/var/log/clamav/freshclam.log
ReceiveTimeout 0
```

`/opt/homebrew/etc/clamav/clamd.conf`
```
DatabaseDirectory /opt/homebrew/var/lib/clamav
LogFile /opt/homebrew/var/log/clamav/clamd.log
PidFile /opt/homebrew/var/run/clamav/clamd.pid
TCPSocket 3310
TCPAddr 127.0.0.1
LocalSocket /opt/homebrew/var/run/clamav/clamd.sock
```

```bash
mkdir -p /opt/homebrew/var/run/clamav /opt/homebrew/var/log/clamav
```

## 3. Signaturen laden & Daemon starten
```bash
freshclam --config-file=/opt/homebrew/etc/clamav/freshclam.conf
clamd     --config-file=/opt/homebrew/etc/clamav/clamd.conf
```
> Hinweis: `ERROR: NULL X509 store` bei freshclam ist unter Homebrew kosmetisch —
> die Datenbanken werden intern per Signatur verifiziert ("Database test passed").

## 4. Über Avox verifizieren
```bash
# TCP (Default)
cargo run -p avox-service -- ping        # -> "clamd: PONG (erreichbar)"
cargo run -p avox-service -- version

# Unix-Socket
AVOX_CLAMD_ADDR=/opt/homebrew/var/run/clamav/clamd.sock \
  cargo run -p avox-service -- ping

# EICAR-Testdatei (offizielle, ungefährliche AV-Testdatei)
mkdir -p /tmp/avox-test
curl -fsSL https://secure.eicar.org/eicar.com.txt -o /tmp/avox-test/eicar.txt
cargo run -p avox-service -- scan /tmp/avox-test   # -> Fund: Eicar-Test-Signature

# Integrationstest der Engine gegen laufenden clamd
AVOX_CLAMD_ADDR=127.0.0.1:3310 cargo test -p avox-engine -- --ignored
```

Erwartetes Ergebnis: `eicar.txt` wird als `Eicar-Test-Signature` erkannt (Exit-Code 1),
saubere Dateien werden ignoriert.
