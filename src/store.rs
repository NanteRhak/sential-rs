//! Keeping the history on disk, so it survives a restart.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// One sample, as it is kept on disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredSample {
    pub timestamp: u64,
    pub cpu_percent: f64,
    pub memory_percent: f64,
    pub load_avg_1: f64,
    pub disk_read_bytes_per_sec: u64,
    pub disk_write_bytes_per_sec: u64,
    pub network_rx_bytes_per_sec: u64,
    pub network_tx_bytes_per_sec: u64,
    /// The busiest processes at that moment.
    pub processes: Vec<StoredProcess>,
}

/// A process, as it is kept on disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredProcess {
    pub pid: u32,
    pub name: String,
    pub cpu_percent: f64,
    pub mem_percent: f64,
}

/// The history file.
pub struct Store {
    path: PathBuf,
}

impl Store {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// Where the history is kept.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Add a sample.
    ///
    /// The write is one line, so it is cheap; the caller decides how often to
    /// write at all.
    pub fn append(&self, sample: &StoredSample) -> Result<(), String> {
        use std::io::Write;
        if let Some(parent) = self.path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .map_err(|error| format!("{}: {error}", self.path.display()))?;
        let line = serde_json::to_string(sample).map_err(|error| error.to_string())?;
        writeln!(file, "{line}").map_err(|error| format!("{}: {error}", self.path.display()))
    }

    /// Read the samples back, oldest first.
    ///
    /// A line that cannot be read is skipped: a truncated history should not
    /// hide the samples that are readable.
    pub fn read(&self) -> Vec<StoredSample> {
        let Ok(text) = std::fs::read_to_string(&self.path) else {
            return Vec::new();
        };
        text.lines()
            .filter_map(|line| serde_json::from_str::<StoredSample>(line).ok())
            .collect()
    }

    /// How much space the history takes.
    pub fn size(&self) -> u64 {
        std::fs::metadata(&self.path)
            .map(|meta| meta.len())
            .unwrap_or(0)
    }
}

/// The history file for a data directory.
pub fn history_path(data_dir: &Path) -> PathBuf {
    data_dir.join("history.jsonl")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(timestamp: u64) -> StoredSample {
        StoredSample {
            timestamp,
            cpu_percent: 10.0,
            memory_percent: 20.0,
            load_avg_1: 0.5,
            disk_read_bytes_per_sec: 0,
            disk_write_bytes_per_sec: 0,
            network_rx_bytes_per_sec: 0,
            network_tx_bytes_per_sec: 0,
            processes: vec![StoredProcess {
                pid: 1,
                name: "init".into(),
                cpu_percent: 1.0,
                mem_percent: 0.1,
            }],
        }
    }

    #[test]
    fn samples_round_trip_through_the_file() {
        let path =
            std::env::temp_dir().join(format!("sentinel-store-{}.jsonl", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let store = Store::new(&path);

        store.append(&sample(1)).unwrap();
        store.append(&sample(2)).unwrap();
        let read = store.read();
        let _ = std::fs::remove_file(&path);

        assert_eq!(read.len(), 2);
        assert_eq!(read[0].timestamp, 1);
        assert_eq!(read[1].processes[0].name, "init");
    }

    #[test]
    fn a_history_that_is_not_there_is_empty() {
        let store = Store::new("/definitely/not/a/directory/history.jsonl");

        assert!(store.read().is_empty());
    }

    #[test]
    fn a_truncated_line_is_skipped() {
        let path =
            std::env::temp_dir().join(format!("sentinel-store-bad-{}.jsonl", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let store = Store::new(&path);
        store.append(&sample(1)).unwrap();
        let mut text = std::fs::read_to_string(&path).unwrap();
        text.push_str("{not json\n");
        std::fs::write(&path, text).unwrap();

        let read = store.read();
        let _ = std::fs::remove_file(&path);

        assert_eq!(read.len(), 1);
    }
}
