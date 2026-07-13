# app/ — Avox GUI (Tauri)

Platzhalter. Der grafische Frontend (Tauri: Rust-Backend + Web-Frontend) entsteht
in **Meilenstein M3** (siehe `../PLAN.md`).

Geplante Struktur:
```
app/
├── src-tauri/   # Rust-Backend, spricht avox-service über IPC an
└── src/         # UI (Framework-Entscheidung in M3, z. B. SvelteKit/React)
```

Bewusst noch nicht Teil des Cargo-Workspaces, damit `cargo build` ohne Node/Tauri
grün bleibt.
