# platform/ — plattformspezifische Adapter

Platzhalter für OS-spezifischen Code, v. a. **Echtzeitschutz (On-Access)** —
der aufwändigste Teil, stufenweise umgesetzt (siehe `../PLAN.md` §5).

```
platform/
├── linux/     # systemd-Unit, fanotify / clamonacc-Adapter  (zuerst)
├── macos/     # Endpoint Security System-Extension, Entitlement
└── windows/   # Dienst-Registrierung, Minifilter-Treiber / AMSI
```

Die enthaltene `linux/avox-service.service` ist eine Beispiel-systemd-Unit für den
Dienst-Skelett-Stand.

## Autostart des Dienstes

**Linux (systemd):**
```bash
sudo cp platform/linux/avox-service.service /etc/systemd/system/
sudo systemctl enable --now avox-service
```

**macOS (launchd, LaunchAgent — Start beim Login):**
```bash
cp platform/macos/org.avox.service.plist ~/Library/LaunchAgents/
launchctl load ~/Library/LaunchAgents/org.avox.service.plist
```
Binary-Pfad in der Plist ggf. anpassen. Für echten Systemschutz gehört der Dienst
später als **LaunchDaemon** (root) nach `/Library/LaunchDaemons`.

**GUI-Autostart & Tray:** Die GUI besitzt ein **System-Tray-Icon** (Öffnen/Beenden).
GUI-Autostart beim Login: die App zu den Anmeldeobjekten hinzufügen bzw. später über
`tauri-plugin-autostart` (Folgeschritt).
