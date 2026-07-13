//! Quarantäne-Grundgerüst.
//!
//! Sicherer Default: befallene Dateien werden **verschoben**, nicht gelöscht, und
//! in einem Index (`index.jsonl`) mit Ursprungspfad protokolliert — Voraussetzung
//! für spätere Wiederherstellung (M3). Verschlüsselung der Quarantäne folgt in
//! einer späteren Stufe (siehe PLAN.md).

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// Ein Eintrag im Quarantäne-Index.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuarantineEntry {
    /// Eindeutige ID (auch der Dateiname in der Quarantäne).
    pub id: String,
    /// Ursprünglicher Pfad der Datei.
    pub original_path: PathBuf,
    /// Zeitpunkt (Unix-Sekunden).
    pub quarantined_at: u64,
}

/// Verwaltet ein Quarantäneverzeichnis.
pub struct Quarantine {
    dir: PathBuf,
}

impl Quarantine {
    /// Öffnet (und erstellt bei Bedarf) das Quarantäneverzeichnis.
    pub fn new(dir: impl Into<PathBuf>) -> io::Result<Self> {
        let dir = dir.into();
        fs::create_dir_all(&dir)?;
        Ok(Self { dir })
    }

    /// Verschiebt eine Datei in die Quarantäne und protokolliert sie.
    pub fn quarantine(&self, path: &Path) -> io::Result<QuarantineEntry> {
        let original = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let stem = path
            .file_name()
            .map(|n| n.to_string_lossy().replace('/', "_"))
            .unwrap_or_else(|| "datei".into());
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0);
        let id = format!("{now}-{nanos}-{stem}");
        let dest = self.dir.join(&id);

        move_file(path, &dest)?;

        let entry = QuarantineEntry {
            id,
            original_path: original,
            quarantined_at: now,
        };
        self.append_index(&entry)?;
        Ok(entry)
    }

    /// Hängt einen Eintrag an den Index (`index.jsonl`) an.
    fn append_index(&self, entry: &QuarantineEntry) -> io::Result<()> {
        let index = self.dir.join("index.jsonl");
        let mut f = OpenOptions::new().create(true).append(true).open(index)?;
        let mut line = serde_json::to_vec(entry)?;
        line.push(b'\n');
        f.write_all(&line)
    }
}

/// Löscht eine Datei endgültig.
pub fn delete(path: &Path) -> io::Result<()> {
    fs::remove_file(path)
}

/// Verschiebt eine Datei; fällt bei geräteübergreifenden Grenzen auf Kopieren+Löschen zurück.
fn move_file(from: &Path, to: &Path) -> io::Result<()> {
    match fs::rename(from, to) {
        Ok(()) => Ok(()),
        Err(_) => {
            // z. B. EXDEV (anderes Dateisystem): kopieren, dann Quelle entfernen.
            fs::copy(from, to)?;
            fs::remove_file(from)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quarantine_moves_file_and_records_index() {
        let base = std::env::temp_dir().join(format!("avox-qtest-{}", std::process::id()));
        let src = base.join("evil.txt");
        let qdir = base.join("quarantine");
        fs::create_dir_all(&base).unwrap();
        fs::write(&src, b"boese").unwrap();

        let q = Quarantine::new(&qdir).unwrap();
        let entry = q.quarantine(&src).unwrap();

        assert!(!src.exists(), "Quelldatei sollte verschoben sein");
        assert!(
            qdir.join(&entry.id).exists(),
            "Datei sollte in Quarantäne liegen"
        );
        assert!(qdir.join("index.jsonl").exists(), "Index sollte existieren");

        fs::remove_dir_all(&base).ok();
    }
}
