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

/// Append a value to a file, one JSON object per line.
pub fn append_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    use std::io::Write;
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|error| format!("{}: {error}", path.display()))?;
    let line = serde_json::to_string(value).map_err(|error| error.to_string())?;
    writeln!(file, "{line}").map_err(|error| format!("{}: {error}", path.display()))
}

/// The history file.
pub struct Store {
    path: PathBuf,
    /// How much space the file may take before the oldest samples are dropped.
    max_bytes: u64,
}

impl Store {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            max_bytes: 10 * 1024 * 1024,
        }
    }

    /// How much space the file may take.
    pub fn with_max_bytes(mut self, max_bytes: u64) -> Self {
        self.max_bytes = max_bytes;
        self
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
        append_json(&self.path, sample)?;
        self.trim()
    }

    /// Drop the oldest samples until the file fits within the limit.
    fn trim(&self) -> Result<(), String> {
        if self.size() <= self.max_bytes {
            return Ok(());
        }
        let text = std::fs::read_to_string(&self.path).map_err(|error| error.to_string())?;
        let lines: Vec<&str> = text.lines().collect();
        let mut start = 0;
        let mut kept = text.len() as u64;
        // The newest sample is always kept, even when it is over the limit.
        while start + 1 < lines.len() && kept > self.max_bytes {
            kept -= lines[start].len() as u64 + 1;
            start += 1;
        }
        // The trailing newline matters: the next sample is appended to the
        // end of the file, and would otherwise join this line.
        let kept = format!("{}\n", lines[start..].join("\n"));
        let temporary = self.path.with_extension("tmp");
        std::fs::write(&temporary, kept).map_err(|error| error.to_string())?;
        std::fs::rename(&temporary, &self.path).map_err(|error| error.to_string())
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

impl StoredSample {
    /// A sample of the machine, as it is kept on disk.
    pub fn of(
        sample: &crate::core::metrics::SystemSample,
        processes: &[crate::core::process::ProcessSample],
    ) -> StoredSample {
        let memory = &sample.memory;
        let memory_percent = crate::core::format::percent(memory.used_bytes, memory.total_bytes);
        let mut processes: Vec<StoredProcess> = processes
            .iter()
            .map(|process| StoredProcess {
                pid: process.pid,
                name: process.name.clone(),
                cpu_percent: process.cpu_percent,
                mem_percent: process.mem_percent,
            })
            .collect();
        processes.sort_by(|a, b| {
            b.cpu_percent
                .partial_cmp(&a.cpu_percent)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        processes.truncate(5);
        StoredSample {
            timestamp: sample.timestamp,
            cpu_percent: sample.cpu.total_percent,
            memory_percent,
            load_avg_1: sample.cpu.load_avg_1,
            disk_read_bytes_per_sec: sample.disk.read_bytes_per_sec,
            disk_write_bytes_per_sec: sample.disk.write_bytes_per_sec,
            network_rx_bytes_per_sec: sample.network.rx_bytes_per_sec,
            network_tx_bytes_per_sec: sample.network.tx_bytes_per_sec,
            processes,
        }
    }
}

/// Writes the history in batches, so persisting it costs nothing per tick.
pub struct HistoryWriter {
    store: Store,
    pending: Vec<StoredSample>,
    last_flush: std::time::Instant,
    interval: std::time::Duration,
    /// How many samples are written before the batch is sent, whatever the
    /// clock says.
    batch_size: usize,
}

impl HistoryWriter {
    pub fn new(store: Store, interval: std::time::Duration) -> Self {
        Self {
            store,
            pending: Vec::new(),
            last_flush: std::time::Instant::now(),
            interval,
            batch_size: 10,
        }
    }

    /// Keep a sample for later.
    pub fn push(&mut self, sample: StoredSample) {
        self.pending.push(sample);
    }

    /// Write the samples that have piled up.
    pub fn flush(&mut self) -> Result<(), String> {
        self.last_flush = std::time::Instant::now();
        let pending = std::mem::take(&mut self.pending);
        for sample in pending {
            self.store.append(&sample)?;
        }
        Ok(())
    }

    /// Write the samples when it is time to.
    pub fn flush_if_due(&mut self) -> Result<(), String> {
        if self.last_flush.elapsed() < self.interval && self.pending.len() < self.batch_size {
            return Ok(());
        }
        self.flush()
    }

    /// How many samples are waiting to be written.
    pub fn pending(&self) -> usize {
        self.pending.len()
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
    fn the_oldest_samples_are_dropped_when_the_file_is_full() {
        let path =
            std::env::temp_dir().join(format!("sentinel-limit-{}.jsonl", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let store = Store::new(&path).with_max_bytes(200);

        for timestamp in 0..10 {
            store.append(&sample(timestamp)).unwrap();
        }
        let read = store.read();
        let _ = std::fs::remove_file(&path);

        assert!(read.len() < 10, "{} samples were kept", read.len());
        assert!(!read.is_empty(), "everything was dropped");
        // The newest samples are the ones that are kept.
        assert_eq!(read.last().unwrap().timestamp, 9);
    }

    #[test]
    fn writes_are_batched_rather_than_done_one_at_a_time() {
        let path =
            std::env::temp_dir().join(format!("sentinel-batch-{}.jsonl", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let mut writer = HistoryWriter::new(Store::new(&path), std::time::Duration::from_secs(60));

        writer.push(sample(1));
        writer.push(sample(2));
        assert_eq!(writer.pending(), 2);
        assert!(Store::new(&path).read().is_empty());

        writer.flush().unwrap();
        let read = Store::new(&path).read();
        let _ = std::fs::remove_file(&path);

        assert_eq!(read.len(), 2);
        assert_eq!(writer.pending(), 0);
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
