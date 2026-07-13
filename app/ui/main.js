"use strict";

// Tauri v2 stellt `invoke` global bereit (withGlobalTauri: true).
const invoke = window.__TAURI__.core.invoke;

const $ = (id) => document.getElementById(id);

function log(msg, kind) {
  const li = document.createElement("li");
  const t = document.createElement("span");
  t.className = "t";
  t.textContent = new Date().toLocaleTimeString();
  const m = document.createElement("span");
  if (kind) m.className = kind;
  m.textContent = msg;
  li.append(t, m);
  const list = $("log");
  list.prepend(li);
  while (list.children.length > 50) list.lastChild.remove();
}

function setStatus(ok, text) {
  const pill = $("status-pill");
  pill.textContent = text;
  pill.className = "pill " + (ok === null ? "pill-unknown" : ok ? "pill-ok" : "pill-bad");
}

// --- Dashboard aktualisieren ---
async function refreshStatus() {
  try {
    await invoke("service_ping");
    $("service-state").textContent = "verbunden";
    setStatus(true, "Geschützt");
  } catch (e) {
    $("service-state").textContent = "nicht erreichbar";
    $("service-sub").textContent = String(e);
    setStatus(false, "Nicht verbunden");
    log(String(e), "err");
    return;
  }
  try {
    const v = await invoke("get_version");
    $("clamd-version").textContent = v.clamd;
    $("service-sub").textContent = "avox-service " + v.service;
  } catch (e) {
    $("clamd-version").textContent = "unbekannt";
    log(String(e), "err");
  }
}

// --- Scan ---
async function doScan() {
  const path = $("scan-path").value.trim();
  if (!path) {
    log("Bitte einen Pfad eingeben.", "err");
    return;
  }
  const btn = $("btn-scan");
  btn.disabled = true;
  btn.textContent = "Scannt…";
  const box = $("scan-result");
  box.hidden = false;
  box.className = "result spin";
  box.innerHTML = "<h3>Scan läuft…</h3>";
  try {
    const r = await invoke("scan", { path });
    renderScan(r, path);
    log(`Scan: ${path} — geprüft ${r.scanned}, Funde ${r.findings.length}`,
        r.findings.length ? "err" : "ok");
    if (r.findings.length) refreshQuarantine();
  } catch (e) {
    box.className = "result infected";
    box.innerHTML = "<h3>Scan fehlgeschlagen</h3>";
    box.append(document.createTextNode(String(e)));
    log(String(e), "err");
  } finally {
    btn.disabled = false;
    btn.textContent = "Scannen";
  }
}

function renderScan(r, path) {
  const box = $("scan-result");
  const infected = r.findings.length > 0;
  box.className = "result " + (infected ? "infected" : "clean");
  box.innerHTML = "";
  const h = document.createElement("h3");
  h.textContent = infected
    ? `${r.findings.length} Bedrohung(en) gefunden — ${r.scanned} Datei(en) geprüft`
    : `Sauber — ${r.scanned} Datei(en) geprüft`;
  box.appendChild(h);
  for (const f of r.findings) {
    const row = document.createElement("div");
    row.className = "finding";
    const left = document.createElement("div");
    const p = document.createElement("div");
    p.className = "finding-path";
    p.textContent = f.path;
    const s = document.createElement("div");
    s.className = "finding-sig";
    s.textContent = f.signature;
    left.append(p, s);
    const btn = document.createElement("button");
    btn.className = "btn btn-ghost btn-small";
    btn.textContent = "In Quarantäne";
    btn.addEventListener("click", () => quarantineOne(f.path, btn));
    row.append(left, btn);
    box.appendChild(row);
  }
}

async function quarantineOne(path, btn) {
  btn.disabled = true;
  try {
    const detail = await invoke("quarantine_file", { path });
    log("Quarantäne: " + detail, "ok");
    btn.textContent = "✓ verschoben";
    refreshQuarantine();
  } catch (e) {
    btn.disabled = false;
    log(String(e), "err");
  }
}

// --- Quarantäne ---
async function refreshQuarantine() {
  const box = $("quarantine-list");
  try {
    const entries = await invoke("list_quarantine");
    if (!entries.length) {
      box.innerHTML = '<p class="muted">Quarantäne ist leer.</p>';
      return;
    }
    box.innerHTML = "";
    for (const e of entries) {
      const row = document.createElement("div");
      row.className = "qrow";
      const left = document.createElement("div");
      const p = document.createElement("div");
      p.className = "qpath";
      p.textContent = e.original_path;
      const meta = document.createElement("div");
      meta.className = "qmeta";
      meta.textContent = "ID " + e.id;
      left.append(p, meta);
      const btn = document.createElement("button");
      btn.className = "btn btn-ghost btn-small";
      btn.textContent = "Wiederherstellen";
      btn.addEventListener("click", () => restoreOne(e.id, btn));
      row.append(left, btn);
      box.appendChild(row);
    }
  } catch (e) {
    box.innerHTML = '<p class="muted"></p>';
    box.firstChild.textContent = String(e);
    log(String(e), "err");
  }
}

async function restoreOne(id, btn) {
  btn.disabled = true;
  try {
    const detail = await invoke("restore", { id });
    log("Wiederhergestellt: " + detail, "ok");
    refreshQuarantine();
  } catch (e) {
    btn.disabled = false;
    log(String(e), "err");
  }
}

// --- Signaturen aktualisieren ---
async function updateSignatures() {
  const btn = $("btn-update");
  btn.disabled = true;
  const old = btn.textContent;
  btn.textContent = "…";
  try {
    const summary = await invoke("update_signatures");
    $("sig-state").textContent = "aktuell";
    log("Signaturen: " + summary, "ok");
  } catch (e) {
    $("sig-state").textContent = "Fehler";
    log(String(e), "err");
  } finally {
    btn.disabled = false;
    btn.textContent = old;
  }
}

// --- Verdrahtung ---
window.addEventListener("DOMContentLoaded", () => {
  $("btn-scan").addEventListener("click", doScan);
  $("scan-path").addEventListener("keydown", (e) => {
    if (e.key === "Enter") doScan();
  });
  $("btn-update").addEventListener("click", updateSignatures);
  $("btn-refresh-q").addEventListener("click", refreshQuarantine);

  log("Avox gestartet.");
  refreshStatus();
  refreshQuarantine();
});
