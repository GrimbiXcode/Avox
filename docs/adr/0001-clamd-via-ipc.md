# ADR 0001 — ClamAV-Anbindung über clamd-IPC statt libclamav-Linken

- **Status:** Angenommen
- **Datum:** 2026-07-13

## Kontext
Avox braucht die ClamAV-Scan-Engine. Zwei Wege: (a) `libclamav` (C) direkt via
FFI in Avox linken, oder (b) den `clamd`-Daemon als separaten Prozess über einen
Socket ansprechen.

## Entscheidung
Wir sprechen **`clamd` über IPC** an (Option b).

## Begründung
- **Lizenz:** ClamAV ist GPLv2. Direktes Linken erzeugt eine GPL-Ableitung mit
  entsprechenden Pflichten. Prozesstrennung über IPC entkoppelt diese Frage.
- **Architektur:** Klare Prozessgrenze zwischen Avox und der Engine; Abstürze der
  Engine reißen die GUI nicht mit.
- **Deployment:** clamd/freshclam sind auf allen Zielplattformen paketiert
  verfügbar; keine C-Toolchain-Abhängigkeit im Avox-Build.

## Konsequenzen
- Mehr Latenz pro Scan durch IPC (für den Anwendungsfall vernachlässigbar).
- Avox setzt einen laufenden `clamd` voraus (vom Installer/Service verwaltet).
- Protokoll-Teilmenge (PING, VERSION, CONTSCAN, später INSTREAM) selbst implementiert
  in `avox-engine`.
- Avox wird dennoch bevorzugt unter **GPL-2.0-only** veröffentlicht (Community-Kompatibilität).
