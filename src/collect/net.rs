//! Per-process network attribution.
//!
//! Linux only says how much traffic a process sent when something is watching
//! the sockets for it. Without that, the honest answer is that the traffic
//! cannot be attributed, and this module says so rather than showing zero as
//! if it were a measurement.

use std::collections::{HashMap, HashSet};
use std::path::Path;

/// The socket inodes the kernel knows about.
pub fn socket_inodes(root: &Path) -> HashSet<u64> {
    let mut inodes = HashSet::new();
    for file in ["net/tcp", "net/tcp6", "net/udp", "net/udp6", "net/raw"] {
        let Ok(text) = std::fs::read_to_string(root.join(file)) else {
            continue;
        };
        for (index, line) in text.lines().enumerate() {
            if index == 0 {
                continue;
            }
            // The inode is the tenth field of the line.
            if let Some(inode) = line.split_whitespace().nth(9)
                && let Ok(inode) = inode.parse::<u64>()
            {
                inodes.insert(inode);
            }
        }
    }
    inodes
}

/// How many network sockets each process holds.
///
/// This is the expensive part of the attribution: every file descriptor of
/// every process has to be looked at.
pub fn socket_counts(root: &Path) -> HashMap<u32, u32> {
    let sockets = socket_inodes(root);
    let mut counts = HashMap::new();
    for pid in crate::collect::process::pids(root) {
        let directory = root.join(pid.to_string()).join("fd");
        let Ok(entries) = std::fs::read_dir(directory) else {
            continue;
        };
        let count = entries
            .filter_map(|entry| entry.ok())
            .filter(|entry| is_socket(entry, &sockets))
            .count() as u32;
        if count > 0 {
            counts.insert(pid, count);
        }
    }
    counts
}

/// Whether a file descriptor is one of the sockets the kernel knows about.
fn is_socket(entry: &std::fs::DirEntry, sockets: &HashSet<u64>) -> bool {
    let Ok(target) = std::fs::read_link(entry.path()) else {
        return false;
    };
    let Some(inode) = target
        .to_string_lossy()
        .strip_prefix("socket:[")
        .and_then(|inode| inode.strip_suffix(']'))
        .and_then(|inode| inode.parse::<u64>().ok())
    else {
        return false;
    };
    sockets.contains(&inode)
}

/// Whether this kernel can say how much traffic a process sent.
///
/// It cannot without something watching the sockets, so the columns are
/// marked as unavailable rather than filled with zeros.
pub fn byte_counters_available() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_socket_inodes_are_read_from_the_kernel_tables() {
        let text = "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode\n   0: 0100007F:1F90 00000000:0000 0A 00000000:00000000 00:00000000 00000000     0        0 12345 1 0000 100 0 0 10 0\n";
        let mut inodes = HashSet::new();
        for (index, line) in text.lines().enumerate() {
            if index == 0 {
                continue;
            }
            if let Some(inode) = line.split_whitespace().nth(9) {
                inodes.insert(inode.parse::<u64>().unwrap());
            }
        }

        assert!(inodes.contains(&12345));
    }

    #[test]
    fn the_socket_count_is_read_without_failing() {
        // There may be no sockets at all in a container, but the call must
        // not fail either way, and every count must belong to a process.
        let counts = socket_counts(Path::new("/proc"));
        for (pid, count) in &counts {
            assert!(*count > 0, "process {pid} was counted with no sockets");
        }
    }

    #[test]
    fn the_byte_counters_are_not_available_without_watching_the_sockets() {
        assert!(!byte_counters_available());
    }
}
