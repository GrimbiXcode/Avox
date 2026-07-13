# Avox — Projektplan

> **Avox** — ein benutzerfreundlicher, quelloffener Antivirus auf Basis von **ClamAV**
> mit grafischer Oberfläche für **Windows, macOS und Linux** auf **x86-64 (amd64),
> ARM64 (aarch64)** und **32-Bit (x86 / armhf)**.

---

## 1. Vision & Ziele

**Problem:** ClamAV ist eine starke Open-Source-Engine, aber ohne komfortable GUI.
Bestehende Frontends (ClamTk, ClamWin) sind veraltet, plattform-fragmentiert und
UX-schwach. Es fehlt ein modernes, einheitliches, wartbares Produkt.

**Vision:** Avox macht ClamAV für Endanwender:innen so einfach wie kommerzielle AV —
mit klarer Oberfläche, Echtzeitschutz, automatischen Updates und sinnvollen Defaults —
und bleibt dabei vollständig quelloffen und transparent.

**Leitprinzipien**
- **Ein Code, alle Plattformen** — gemeinsamer Kern, plattformspezifische Adapter nur wo nötig.
- **Sichere Defaults** — funktioniert direkt nach der Installation ohne Konfiguration.
- **Transparenz** — keine Telemetrie ohne Opt-in, alles nachvollziehbar.
- **Barrierearm** — verständliche Sprache, Mehrsprachigkeit (de/en zuerst).

**Nicht-Ziele (bewusst außerhalb Scope, mindestens v1)**
- Eigene Signatur-/Erkennungsforschung (wir nutzen ClamAV-Feeds).
- Cloud-Sandboxing, EDR/Enterprise-Flottenmanagement.
- Firewall / VPN / „Security-Suite"-Featurewucher.

---

## 2. Architektur

Avox trennt strikt zwischen **privilegiertem Dienst** und **unprivilegierter GUI** —
Standard für AV-Software, notwendig für Echtzeit-Scanning und Quarantäne.

```
┌────────────────────────────────────────────────────────────┐
│                        Avox GUI (User)                     │
│   Dashboard · Scan · Quarantäne · Verlauf · Einstellungen  │
└───────────────────────────┬────────────────────────────────┘
                            │  IPC (lokaler Socket / Named Pipe,
                            │       authentifiziert, JSON-RPC)
┌───────────────────────────┴────────────────────────────────┐
│                   Avox Service (privilegiert)              │
│  Scan-Orchestrierung · Zeitpläne · Quarantäne · Updates    │
│  Echtzeitschutz-Steuerung · Event-/Report-Log             │
└──────┬───────────────────────┬───────────────┬─────────────┘
       │                       │               │
 ┌─────┴──────┐        ┌───────┴──────┐  ┌──────┴───────┐
 │ libclamav /│        │  freshclam    │  │ On-Access-   │
 │  clamd     │        │ (Signaturen)  │  │ Adapter/OS   │
 └────────────┘        └───────────────┘  └──────────────┘
```

### Schichten
| Schicht | Aufgabe | Technologie |
|---|---|---|
| **Engine** | Datei-Scan, Signaturabgleich | **`clamd`-Daemon via IPC** (entkoppelt, nicht gelinkt) |
| **Signaturen** | Updates der Virendefinitionen | `freshclam` + optionale Feeds |
| **Service** | Orchestrierung, Rechte, Quarantäne, Zeitpläne | Rust (eigenständiger Daemon/Dienst) |
| **IPC** | Trennung Dienst ↔ GUI | JSON-RPC über UDS / Named Pipe |
| **GUI** | Bedienoberfläche | **Tauri** (Rust-Backend + Web-Frontend) |
| **Echtzeitschutz** | On-Access-Scanning | pro OS unterschiedlich (siehe §5) |

### Warum Tauri + Rust?
- **Leichtgewichtig** (nativer WebView statt gebündeltem Chromium wie Electron) —
  wichtig für ARM/32-Bit und schwächere Geräte.
- **Rust-FFI** bindet `libclamav` (C) sauber an; ein Sprach-Ökosystem für Service + GUI-Backend.
- Sehr gute **Cross-Compilation** und Paketierung für alle Zielplattformen.
- **Alternative geprüft:** Qt/C++ (näher an ClamAVs C-Welt, aber schwergewichtigere GUI-Entwicklung).
  Flutter/Electron verworfen (Größe, ARM/32-Bit-Reibung). → Entscheidung: **Tauri**, Qt als Fallback.

---

## 3. Funktionsumfang

### MVP (v0.1 – v1.0)
- **On-Demand-Scan**: Schnellscan, Vollscan, benutzerdefinierter Pfad, Datei/Ordner per Rechtsklick.
- **Ergebnisse & Aktionen**: Fund anzeigen → Quarantäne / Löschen / Ignorieren (Whitelist).
- **Quarantäne**: sichere, verschlüsselte Ablage; Wiederherstellen; endgültig löschen.
- **Signatur-Updates**: automatisch via `freshclam`, manueller Trigger, Statusanzeige.
- **Zeitgesteuerte Scans**: täglich/wöchentlich, konfigurierbar.
- **Dashboard**: Schutzstatus, letzter Scan, Signaturalter, Handlungsempfehlungen.
- **Verlauf/Log**: Scans und Funde nachvollziehbar.
- **Einstellungen**: Ausschlüsse, Scan-Tiefe (Archive, E-Mail), Ressourcenlimit.
- **i18n**: Deutsch + Englisch.

### v1.x – v2.0
- **Echtzeitschutz (On-Access)** pro Plattform (siehe §5) — technisch anspruchsvollster Teil.
- Autostart / Tray-Icon / Benachrichtigungen.
- PUA-Erkennung (Potentially Unwanted Applications), konfigurierbar.
- Zusätzliche Signatur-Feeds (z. B. YARA-Regeln, Community-DBs).
- Barrierefreiheit & Screenreader-Support, weitere Sprachen.

### Später / optional
- CLI-Companion für Power-User/Server (`avox scan …`).
- Rescue/Boot-Scan-Medium.
- Signierte Reports/Export für Compliance.

---

## 4. Plattform- & Architektur-Matrix

| OS | amd64 | arm64 | 32-Bit | Paketformat | Besonderheit |
|---|:--:|:--:|:--:|---|---|
| **Windows** 10/11 | ✅ | ✅ | ⚠️ x86 | MSI / EXE (WiX) | Dienst, Code-Signing, ggf. Minifilter-Treiber |
| **macOS** 12+ | ✅ | ✅ (Apple Silicon) | — | `.dmg` / `.pkg` | Notarisierung, Endpoint Security Entitlement |
| **Linux** | ✅ | ✅ | ⚠️ armhf/x86 | `.deb`, `.rpm`, AppImage, Flatpak | systemd-Service, fanotify |

Legende: ✅ Voll · ⚠️ Best-effort/nachrangig · — n/a
> 32-Bit ist erklärtes Ziel, aber niedrigere Priorität als 64-Bit. Frühe CI-Builds
> stellen Baubarkeit sicher, volle Testabdeckung folgt später.

---

## 5. Echtzeitschutz — der schwierige Teil (bewusst hervorgehoben)

On-Access-Scanning ist **pro Betriebssystem grundverschieden** und der Hauptaufwandstreiber:

- **Linux:** ClamAVs `clamonacc` über **fanotify** — relativ direkt, gute Grundlage.
- **macOS:** Apples **Endpoint Security Framework** — erfordert Apple-**Entitlement**
  (Antrag bei Apple), System-Extension, Notarisierung. Hohe Hürde, aber der einzige
  sanktionierte Weg.
- **Windows:** **Minifilter-Treiber** (WDK) für echten On-Access-Schutz, alternativ
  Registrierung als Antivirus über **AMSI / Windows Security Center**. Treiberentwicklung
  + **EV-Code-Signing** nötig.

**Konsequenz für den Plan:** MVP liefert zunächst **On-Demand + Zeitplan** auf allen
Plattformen. Echtzeitschutz kommt **stufenweise** (Linux → macOS → Windows), da jeweils
Zertifikate/Entitlements/Treiber erforderlich sind. Das entkoppelt ein nutzbares
Release von den langwierigen Plattform-Freigaben.

---

## 6. Lizenzierung & Recht

- **ClamAV** steht unter **GPLv2**. Bei statischem/dynamischem Linken von `libclamav`
  wird Avox' Ableitung ebenfalls **GPL-pflichtig**.
- **Entscheidung (getroffen):** Avox spricht **`clamd` als separaten Prozess über IPC** an,
  statt `libclamav` zu linken → die GPL-Ableitung wird vermieden, saubere Prozesstrennung,
  einfacheres Deployment. Avox selbst wird dennoch bevorzugt unter **GPLv2** veröffentlicht
  (klare Rechtslage, Community-Kompatibilität).
- **Marke/Name:** „Avox" prüfen (Marken-/Domain-Recherche) — es gibt gleichnamige Produkte.
- **Signatur-Daten:** ClamAV-Feed-Nutzungsbedingungen (freshclam Mirror-Policy, CDN) beachten,
  keine aggressive Update-Frequenz.
- **Haftung/Disclaimer:** AV ohne Gewährleistung; klarer Hinweis, kein 100%-Schutz.

---

## 7. Projektstruktur (Monorepo)

```
avox/
├── crates/
│   ├── avox-engine/     # libclamav/clamd-Bindings, Scan-Abstraktion
│   ├── avox-service/    # privilegierter Daemon: Orchestrierung, Quarantäne, Updates
│   ├── avox-ipc/        # gemeinsame JSON-RPC-Typen (Service ↔ GUI)
│   └── avox-core/       # geteilte Domänenlogik, Konfig, i18n
├── app/                 # Tauri-App (Rust-Backend + Web-Frontend)
│   ├── src-tauri/
│   └── src/             # UI (Framework z. B. SvelteKit/React)
├── platform/
│   ├── windows/         # Dienst-Installer, (später) Minifilter
│   ├── macos/           # Endpoint-Security-Extension, Notarisierung
│   └── linux/           # systemd-Unit, fanotify-Adapter
├── packaging/           # WiX, dmg/pkg, deb/rpm, AppImage, Flatpak
├── docs/                # Doku, ADRs, Handbuch
├── .github/workflows/   # CI/CD (Matrix-Builds)
└── PLAN.md
```

---

## 8. Roadmap & Meilensteine

| Phase | Ziel | Wesentliche Ergebnisse |
|---|---|---|
| **M0 – Setup** (2–3 Wo) | Fundament | Repo, CI-Skelett, ADRs, Tauri-Hello-World, `libclamav`-Bindung als Spike |
| **M1 – Engine-Kern** (3–4 Wo) | Scannen funktioniert headless | `avox-engine` scannt Datei/Ordner, gibt Funde zurück; Unit-Tests mit EICAR |
| **M2 – Service + IPC** (3–4 Wo) | Dienst steuert Scans | Daemon, JSON-RPC, Rechtekonzept, freshclam-Integration |
| **M3 – GUI MVP** (4–6 Wo) | Bedienbares Produkt | Dashboard, Scan-Flow, Ergebnisse, Quarantäne, Updates, Einstellungen, de/en |
| **M4 – Paketierung** (3–4 Wo) | Installierbar überall | deb/rpm/AppImage, dmg/pkg, MSI; Signing-Pipeline (Linux zuerst) |
| **M5 – Beta / v1.0** (4 Wo) | Stabilität | Zeitpläne, Tray/Autostart, Fehlerhärtung, Doku, öffentliche Beta |
| **M6 – Echtzeitschutz** (laufend) | On-Access | Linux (fanotify) → macOS (ES) → Windows (Minifilter/AMSI) |

> Zeitangaben sind grobe Richtwerte für ein kleines Team; Reihenfolge zählt mehr als exakte Dauer.

---

## 9. CI/CD, Qualität & Sicherheit

- **CI-Matrix:** GitHub Actions über {Windows, macOS, Linux} × {amd64, arm64, 32-Bit}.
  ARM-Builds via Cross-Compilation / native Runner / QEMU.
- **Tests:** Unit (Engine mit **EICAR**-Testdatei), Integration (Service↔GUI IPC),
  Smoke-Tests der Pakete, manuelle UI-Checkliste.
- **Sicherheit:** minimale Rechte im Service, gehärtete IPC-Authentifizierung,
  Quarantäne-Isolation, Supply-Chain-Absicherung (`cargo audit`, SBOM, reproduzierbare Builds),
  Code-Signing/Notarisierung.
- **Release:** SemVer, signierte Artefakte, automatische Changelogs, Auto-Update-Kanal (opt-in).

---

## 10. Community & Governance

- **Repo:** GitHub, klare `CONTRIBUTING.md`, Code of Conduct, Issue-/PR-Templates.
- **Doku:** Nutzerhandbuch + Entwickler-Doku; Architecture Decision Records (ADRs).
- **Übersetzungen:** Weblate/Crowdin für Community-i18n.
- **Kommunikation:** Diskussionsforum/Matrix, Roadmap öffentlich, Security-Policy (`SECURITY.md`) mit Meldeprozess.

---

## 11. Zentrale Risiken

| Risiko | Auswirkung | Gegenmaßnahme |
|---|---|---|
| Echtzeitschutz je OS sehr aufwändig (Treiber/Entitlements) | Verzögerung Kernfeature | Stufenweise; MVP ohne On-Access; früh Apple-Entitlement & EV-Cert beantragen |
| GPL-Pflicht durch libclamav | Lizenzzwang | Avox als GPL veröffentlichen **oder** clamd via IPC entkoppeln |
| Code-Signing/Notarisierung (Kosten, Zeit) | Distribution blockiert | Zertifikate früh beschaffen; Linux zuerst releasen |
| 32-Bit-/ARM-Sonderfälle | Build-/Testaufwand | Baubarkeit früh in CI absichern, Priorität nach 64-Bit |
| Name/Marke „Avox" belegt | Rebranding nötig | Frühe Marken-/Domain-Recherche |
| False Positives / Nutzer löscht wichtige Datei | Vertrauensverlust | Quarantäne statt Löschen als Default, Wiederherstellung, klare Warnungen |

---

## 12. Nächste konkrete Schritte

1. **Entscheidungen fixieren:** GUI-Stack (Tauri bestätigen), Lizenz (GPLv2), Namensprüfung.
2. **Repo & CI-Skelett** aufsetzen (Monorepo-Struktur aus §7, leere CI-Matrix).
3. **Spike `libclamav`-Binding** in Rust — EICAR-Datei erkennen (Machbarkeitsnachweis).
4. **ADRs** für die drei Kernentscheidungen (Engine-Anbindung, IPC, GUI) schreiben.
5. Apple-Entitlement- und Code-Signing-Beschaffung **jetzt** anstoßen (lange Vorlaufzeit).
```
