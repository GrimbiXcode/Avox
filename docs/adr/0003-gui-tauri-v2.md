# ADR 0003 — GUI mit Tauri v2 und statischem Vanilla-Frontend

- **Status:** Angenommen
- **Datum:** 2026-07-13

## Kontext
Avox braucht eine plattformübergreifende GUI (Windows/macOS/Linux, amd64/arm64),
die den privilegierten `avox-service` über IPC anspricht.

## Entscheidung
- **Framework:** **Tauri v2** (Rust-Backend + System-WebView).
- **Frontend:** **statisches Vanilla-HTML/CSS/JS** ohne Bundler; `withGlobalTauri`
  aktiviert, sodass `main.js` direkt `window.__TAURI__.core.invoke` nutzt.
- Die GUI-Crate (`app/src-tauri`) ist **eigenständig** (nicht im Cargo-Workspace),
  damit `cargo build --workspace` und die Kern-CI ohne WebView-Systemabhängigkeiten
  grün bleiben.

## Begründung
- Tauri nutzt den nativen WebView statt eines gebündelten Chromium (Electron) →
  kleine Binaries, gut für ARM/schwächere Geräte (vgl. [[0001-clamd-via-ipc]] / PLAN §2).
- Rust-Backend teilt sich das Ökosystem mit `avox-ipc`/`avox-core` — die
  Tauri-Commands sind dünne Adapter über denselben IPC-Transport.
- Kein JS-Bundler senkt die Einstiegshürde und hält den Build reproduzierbar;
  ein Framework (Svelte/React) kann später ergänzt werden, wenn die UI wächst.

## Konsequenzen
- Für `tauri dev`/`build` wird die Tauri-CLI benötigt (`@tauri-apps/cli`), plus die
  Tauri-Systemabhängigkeiten (Linux: webkit2gtk u. a.).
- Ohne Bundler keine npm-Frontend-Build-Kette — bei komplexer UI später nachrüstbar.
- Die GUI enthält keine Scan-Logik; alle Aktionen laufen über den Dienst
  (Privileg-Trennung bleibt gewahrt).
