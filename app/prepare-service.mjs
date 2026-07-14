// Baut `avox-service` (Release) und legt es als Bundle-Ressource der GUI ab.
// Wird von Tauri als `beforeBuildCommand` ausgeführt (vor jedem `tauri build`/`dev`),
// damit die App den Dienst immer mitbringt und selbst starten kann.

import { execSync } from "node:child_process";
import { copyFileSync, mkdirSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { platform } from "node:process";

// Pfade unabhängig vom aktuellen Arbeitsverzeichnis auflösen.
const appDir = dirname(fileURLToPath(import.meta.url)); // …/app
const repoRoot = join(appDir, ".."); // Repo-Wurzel (Workspace)
const ext = platform === "win32" ? ".exe" : "";

console.log("[prepare-service] baue avox-service (release)…");
execSync("cargo build --release -p avox-service", {
  stdio: "inherit",
  cwd: repoRoot,
});

const src = join(repoRoot, "target", "release", `avox-service${ext}`);
const destDir = join(appDir, "src-tauri", "resources");
mkdirSync(destDir, { recursive: true });
copyFileSync(src, join(destDir, `avox-service${ext}`));
console.log(`[prepare-service] abgelegt: src-tauri/resources/avox-service${ext}`);
