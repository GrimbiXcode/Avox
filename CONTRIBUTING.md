# Beiträge zu Avox

Danke für dein Interesse! Avox ist in früher Entwicklung — Beiträge sind willkommen.

## Grundregeln
- Diskutiere größere Änderungen zuerst in einem Issue.
- Ein PR = ein Thema. Halte Diffs überschaubar.
- Neuer Code kommt mit Tests.

## Entwicklungs-Setup
- Rust (stable) via `rustup`.
- Ein laufender `clamd` für Integrationstests.

## Vor jedem PR
```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```
Die CI (siehe `.github/workflows/ci.yml`) führt dieselben Schritte plattformübergreifend aus.

## Commit-Stil
- Aussagekräftige Nachrichten, gern nach [Conventional Commits](https://www.conventionalcommits.org/).

## Architektur-Entscheidungen
Wesentliche Entscheidungen werden als ADR unter `docs/adr/` festgehalten. Neue
grundlegende Entscheidung? Lege ein neues ADR im Format des bestehenden an.
