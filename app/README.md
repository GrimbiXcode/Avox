# app/ — Avox GUI (Tauri v2)

Unprivilegierte grafische Oberfläche. Sie enthält **keine** Scan-Logik, sondern
spricht ausschließlich den privilegierten `avox-service` über die IPC (`avox-ipc`)
an — dieselbe Schnittstelle wie der `call`-Client.

```
app/
├── package.json          # npm-Skripte (tauri dev/build)
├── ui/                   # statisches Frontend (Vanilla JS, kein Bundler)
│   ├── index.html
│   ├── styles.css
│   └── main.js           # ruft Tauri-Commands via window.__TAURI__.core.invoke
└── src-tauri/            # Rust-Backend (Tauri v2)
    ├── Cargo.toml        # eigenständiges Crate, NICHT im Workspace
    ├── tauri.conf.json
    ├── capabilities/
    ├── icons/
    └── src/
        ├── main.rs
        └── lib.rs        # #[tauri::command]s -> avox-ipc -> avox-service
```

## Voraussetzungen
- Rust (stable), Node ≥ 18
- Tauri-CLI: `npm install` im `app/`-Ordner (installiert `@tauri-apps/cli`)
- Systemabhängigkeiten von Tauri v2 (macOS: WebKit ist vorhanden; Linux:
  `webkit2gtk`/`libsoup` etc. — siehe Tauri-Doku)

## Starten (Entwicklung)
```bash
# 1) In einem Terminal den privilegierten Dienst starten
cargo run -p avox-service -- serve            # aus dem Repo-Root

# 2) Im app/-Ordner die GUI im Dev-Modus starten
cd app
npm install
npm run dev                                   # = tauri dev
```

Die GUI verbindet sich mit dem Default-IPC-Endpoint (`/tmp/avox-service.sock`).
Überschreiben mit `AVOX_IPC` (identisch bei Dienst und GUI setzen).

## Nur das Backend kompilieren (ohne CLI)
```bash
cd app/src-tauri && cargo build
```

## Funktionen der GUI
- **Dashboard:** Dienststatus, clamd-Version, Signaturen
- **Scan:** Pfad eingeben → Ergebnis mit Funden; Fund direkt in Quarantäne
- **Quarantäne:** Liste mit Wiederherstellen
- **Signaturen aktualisieren** (freshclam)
- **Aktivitätslog**

> Hinweis: `withGlobalTauri` ist aktiv, daher braucht das Frontend keinen
> JS-Bundler — `main.js` nutzt direkt `window.__TAURI__.core.invoke`.
