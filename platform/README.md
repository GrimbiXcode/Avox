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
