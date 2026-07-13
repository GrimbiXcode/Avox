# ADR 0002 — IPC-Transport & Framing zwischen Service und GUI

- **Status:** Angenommen
- **Datum:** 2026-07-13

## Kontext
GUI (unprivilegiert) und Service (privilegiert) müssen lokal kommunizieren. Zu
klären: Transport und Nachrichtenformat.

## Entscheidung
- **Transport:** **Unix-Domain-Socket** auf Linux/macOS (Zugriffsschutz über
  Dateisystem-Rechte, kein Netzwerk-Exposure); **loopback-TCP** als Fallback (u. a.
  Windows), bis ein Named-Pipe-Transport folgt.
- **Framing:** **line-delimited JSON** — ein JSON-Objekt pro Zeile.
- **Typen:** `Request`/`Response` als serde-Enums in `avox-ipc`, jeweils in einem
  Umschlag mit `id` zur Korrelation.

## Begründung
- Unix-Socket ist der Standard für lokale Privileg-Trennung und lässt sich über
  Dateirechte absichern — kein offener TCP-Port für andere lokale Nutzer.
- Line-delimited JSON ist streamfähig, trivial zu framen und mit Standardwerkzeugen
  (`nc -U`) debugbar — pragmatischer als ein volles JSON-RPC-Framework für den Start.
- Ein `id`-Umschlag hält den Weg zu Mehrfach-/Async-Anfragen offen.

## Konsequenzen
- Windows nutzt vorerst loopback-TCP (nur `127.0.0.1`); Named Pipes sind ein späterer Schritt.
- Kein Backpressure-/Streaming-Fortschritt pro Datei in dieser Stufe (ein Request →
  ein Response). Fortschrittsanzeige beim Scannen folgt später (ggf. Server-Push).
- Authentifizierung/Härtung der Socket-Rechte wird in einer späteren Stufe vertieft.
