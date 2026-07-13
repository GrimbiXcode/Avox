//! Quarantäne.
//!
//! Sicherer Default: befallene Dateien werden **verschoben**, nicht gelöscht, und
//! in einem Index (`index.jsonl`) mit Ursprungspfad protokolliert — Grundlage für
//! die **Wiederherstellung**. Verschlüsselung der Quarantäne folgt in einer
//! späteren Stufe (siehe PLAN.md).

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use avox_core::QuarantineEntry;

/// Verwaltet ein Quarantäneverzeichnis samt Index.
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
        let now = unix_secs();
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

    /// Listet alle aktuell in Quarantäne befindlichen Einträge. Beschädigte
    /// Index-Zeilen werden übersprungen (nicht fatal).
    pub fn list(&self) -> io::Result<Vec<QuarantineEntry>> {
        let content = match fs::read_to_string(self.index_path()) {
            Ok(c) => c,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e),
        };
        let entries = content
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .filter_map(|l| serde_json::from_str::<QuarantineEntry>(l).ok())
            .collect();
        Ok(entries)
    }

    /// Stellt einen Eintrag an seinen Ursprungsort zurück und entfernt ihn aus dem Index.
    pub fn restore(&self, id: &str) -> io::Result<QuarantineEntry> {
        let mut entries = self.list()?;
        let pos = entries.iter().position(|e| e.id == id).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("kein Quarantäne-Eintrag mit id {id}"),
            )
        })?;
        let entry = entries[pos].clone();
        let source = self.dir.join(&entry.id);

        if let Some(parent) = entry.original_path.parent() {
            fs::create_dir_all(parent)?;
        }
        move_file(&source, &entry.original_path)?;

        entries.remove(pos);
        self.write_index(&entries)?;
        Ok(entry)
    }

    fn index_path(&self) -> PathBuf {
        self.dir.join("index.jsonl")
    }

    /// Hängt einen Eintrag an den Index an.
    fn append_index(&self, entry: &QuarantineEntry) -> io::Result<()> {
        let mut f = OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.index_path())?;
        let mut line = serde_json::to_vec(entry)?;
        line.push(b'\n');
        f.write_all(&line)
    }

    /// Schreibt den Index vollständig neu (nach Entfernen eines Eintrags).
    fn write_index(&self, entries: &[QuarantineEntry]) -> io::Result<()> {
        let mut out = Vec::new();
        for e in entries {
            out.extend_from_slice(&serde_json::to_vec(e)?);
            out.push(b'\n');
        }
        fs::write(self.index_path(), out)
    }
}

/// Löscht eine Datei endgültig.
pub fn delete(path: &Path) -> io::Result<()> {
    fs::remove_file(path)
}

fn unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
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
    fn quarantine_then_restore_roundtrip() {
        let base = std::env::temp_dir().join(format!("avox-qtest-{}", std::process::id()));
        let src = base.join("evil.txt");
        let qdir = base.join("quarantine");
        fs::create_dir_all(&base).unwrap();
        fs::write(&src, b"boese").unwrap();

        let q = Quarantine::new(&qdir).unwrap();
        let entry = q.quarantine(&src).unwrap();

        // Nach Quarantäne: Quelle weg, Datei + Index vorhanden, list() sieht 1 Eintrag.
        assert!(!src.exists(), "Quelldatei sollte verschoben sein");
        assert!(
            qdir.join(&entry.id).exists(),
            "Datei sollte in Quarantäne liegen"
        );
        assert_eq!(q.list().unwrap().len(), 1);

        // Wiederherstellen: Datei zurück am Ursprung, aus Quarantäne + Index entfernt.
        let restored = q.restore(&entry.id).unwrap();
        assert_eq!(restored.id, entry.id);
        assert!(src.exists(), "Datei sollte wiederhergestellt sein");
        assert!(
            !qdir.join(&entry.id).exists(),
            "Quarantäne-Datei sollte weg sein"
        );
        assert!(q.list().unwrap().is_empty(), "Index sollte leer sein");

        // Unbekannte id → NotFound.
        assert!(q.restore("gibt-es-nicht").is_err());

        fs::remove_dir_all(&base).ok();
    }
}
