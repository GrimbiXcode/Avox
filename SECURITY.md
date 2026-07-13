# Sicherheitsrichtlinie

Avox ist Sicherheitssoftware — verantwortungsvolle Meldungen sind besonders wichtig.

## Schwachstellen melden
- **Nicht** über öffentliche Issues.
- Bitte über die privaten „Security Advisories" von GitHub oder per E-Mail an das
  Maintainer-Team (Adresse folgt mit dem ersten öffentlichen Release).
- Wir bestätigen den Eingang zeitnah und koordinieren die Offenlegung.

## Geltungsbereich
- `avox-service`, `avox-engine`, `avox-ipc`, `avox-core` und die GUI (`app/`).
- Schwachstellen in ClamAV selbst bitte an das
  [ClamAV-Projekt](https://www.clamav.net/) melden.

## Sicherheitsprinzipien (siehe `PLAN.md` §9)
- Minimale Rechte im Dienst, authentifizierte IPC.
- Quarantäne isoliert, Löschen nie als Default.
- Supply-Chain-Absicherung (`cargo audit`, SBOM), signierte Releases.
