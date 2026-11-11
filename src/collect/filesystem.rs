//! Filesystem capacity, read through `statvfs`.

use std::ffi::CString;

/// Filesystem types worth showing. Pseudo filesystems are noise.
const REAL_FILESYSTEMS: [&str; 11] = [
    "ext2", "ext3", "ext4", "xfs", "btrfs", "zfs", "f2fs", "vfat", "ntfs", "exfat", "overlay",
];

/// The capacity of a mounted filesystem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilesystemCounters {
    pub mount_point: String,
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub available_bytes: u64,
}

/// The mount points of the real filesystems, from `/proc/mounts`.
pub fn mount_points(mounts: &str) -> Vec<String> {
    let mut points = Vec::new();
    for line in mounts.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 3 {
            continue;
        }
        if !REAL_FILESYSTEMS.contains(&fields[2]) {
            continue;
        }
        let mount_point = unescape(fields[1]);
        if !points.contains(&mount_point) {
            points.push(mount_point);
        }
    }
    points
}

/// The capacity of one mounted filesystem.
///
/// `Err` says why the filesystem could not be read, so the caller can note it
/// and carry on with the rest.
pub fn usage(mount_point: &str) -> Result<FilesystemCounters, String> {
    let path =
        CString::new(mount_point).map_err(|_| "mount point contains a NUL byte".to_string())?;
    let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statvfs(path.as_ptr(), &mut stat) } != 0 {
        return Err(format!(
            "{mount_point}: {}",
            std::io::Error::last_os_error()
        ));
    }
    let block = stat.f_frsize as u64;
    let total = stat.f_blocks as u64 * block;
    let free = stat.f_bfree as u64 * block;
    let available = stat.f_bavail as u64 * block;
    Ok(FilesystemCounters {
        mount_point: mount_point.to_string(),
        total_bytes: total,
        used_bytes: total.saturating_sub(free),
        available_bytes: available,
    })
}

/// `/proc/mounts` escapes spaces and other specials as octal.
fn unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        let code: String = chars.by_ref().take(3).collect();
        match u8::from_str_radix(&code, 8) {
            Ok(byte) => out.push(byte as char),
            Err(_) => {
                out.push('\\');
                out.push_str(&code);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_real_filesystems_are_listed() {
        let mounts = "/dev/sda1 / ext4 rw 0 0\nproc /proc proc rw 0 0\ntmpfs /run tmpfs rw 0 0\n/dev/sdb1 /mnt/data xfs rw 0 0\n";

        let points = mount_points(mounts);

        assert_eq!(points, ["/", "/mnt/data"]);
    }

    #[test]
    fn escaped_spaces_in_mount_points_are_decoded() {
        let mounts = "/dev/sdb1 /mnt/my\\040disk ext4 rw 0 0\n";

        let points = mount_points(mounts);

        assert_eq!(points, ["/mnt/my disk"]);
    }

    #[test]
    fn the_root_filesystem_can_be_read() {
        let counters = usage("/").unwrap();

        assert!(counters.total_bytes > 0, "{counters:?}");
        assert!(counters.used_bytes <= counters.total_bytes);
    }

    #[test]
    fn a_mount_point_that_does_not_exist_says_so() {
        let error = usage("/definitely/not/a/mount").unwrap_err();

        assert!(error.contains("/definitely/not/a/mount"), "{error}");
    }
}
