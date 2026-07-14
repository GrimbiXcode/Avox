# packaging/ — Distributionsartefakte

Die Installer werden von **Tauri** aus `app/` erzeugt (`bundle.targets: "all"` in
`app/src-tauri/tauri.conf.json`). Es braucht keine Dateien in diesem Ordner — die
Bundles landen unter `app/src-tauri/target/release/bundle/`.

## Lokal bauen
```bash
cd app
npm ci
npx tauri build          # baut Bundles für die aktuelle Plattform
```

Ergebnisse je Plattform:
| Plattform | Artefakte | Pfad |
|---|---|---|
| macOS | `.app`, `.dmg` | `.../bundle/macos/`, `.../bundle/dmg/` |
| Linux | `.deb`, `.rpm`, `.AppImage` | `.../bundle/deb/`, `rpm/`, `appimage/` |
| Windows | `.msi`, `-setup.exe` | `.../bundle/msi/`, `nsis/` |

> Cross-Plattform-Builds laufen **nicht** lokal — jedes OS baut seine eigenen
> Artefakte. Dafür ist die CI da.

## CI / Release
- **`.github/workflows/app.yml`** — baut die Bundles bei jedem Push/PR auf
  macOS/Linux/Windows und lädt sie als Build-Artefakte hoch (Baubarkeits-Check).
- **`.github/workflows/release.yml`** — bei einem Tag `vX.Y.Z`: baut alle Installer
  via `tauri-action` und hängt sie an einen **GitHub-Release-Entwurf**.

Ein Release auslösen:
```bash
git tag v0.1.0 && git push origin v0.1.0
```

## Signierung / Notarisierung (offen, siehe PLAN.md §6/§9)
Aktuell werden die Bundles **unsigniert** gebaut. Für die Verteilung nötig:
- **macOS:** Developer-ID-Signatur + **Notarisierung** (Apple), Entitlements.
- **Windows:** **EV-Code-Signing** (sonst SmartScreen-Warnung).
- **Linux:** Repo-/Paket-Signatur (GPG) je nach Distribution.

Die Zertifikate/Entitlements haben lange Vorlaufzeiten und sollten früh beschafft
werden. `tauri-action` unterstützt Signatur-Secrets, sobald sie vorliegen.
