//! Capping what a process may use, with cgroups.

use std::path::{Path, PathBuf};

/// The CPU and memory a process is allowed.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Limits {
    /// The share of one processor, as a percentage.
    pub cpu_percent: Option<f64>,
    /// The share of the machine's memory, as a percentage.
    pub memory_percent: Option<f64>,
}

impl Limits {
    pub fn is_empty(&self) -> bool {
        self.cpu_percent.is_none() && self.memory_percent.is_none()
    }
}

/// The cgroup of a process.
pub fn cgroup_dir(root: &Path, pid: u32) -> PathBuf {
    root.join(format!("sentinel-{pid}"))
}

/// Put a process in a cgroup with the given limits.
pub fn apply(root: &Path, pid: u32, limits: &Limits, total_memory: u64) -> Result<String, String> {
    if limits.is_empty() {
        return Err("no limit was given".into());
    }
    let directory = cgroup_dir(root, pid);
    std::fs::create_dir_all(&directory).map_err(|error| {
        permission_error(
            root,
            &directory,
            &format!("cannot create the cgroup: {error}"),
        )
    })?;

    if let Some(cpu) = limits.cpu_percent {
        // The period is 100ms, so the quota is the percentage in tenths.
        let quota = (cpu * 1_000.0).round().max(1_000.0);
        write(&directory.join("cpu.max"), &format!("{quota:.0} 100000"))?;
    }
    if let Some(memory) = limits.memory_percent {
        let bytes = ((total_memory as f64) * memory / 100.0).round().max(1.0);
        write(&directory.join("memory.max"), &format!("{bytes:.0}"))?;
    }

    write(&directory.join("cgroup.procs"), &pid.to_string())?;
    Ok(format!(
        "process {pid} is now in {} with {:?}",
        directory.display(),
        limits
    ))
}

/// Take the limits off a process.
pub fn release(root: &Path, pid: u32) -> Result<String, String> {
    // Moving it back to the root cgroup is what takes the limits off.
    write(&root.join("cgroup.procs"), &pid.to_string())?;
    let directory = cgroup_dir(root, pid);
    if directory.exists() {
        for file in ["cpu.max", "memory.max", "cgroup.procs"] {
            let _ = std::fs::remove_file(directory.join(file));
        }
        std::fs::remove_dir(&directory)
            .map_err(|error| format!("cannot remove {}: {error}", directory.display()))?;
    }
    Ok(format!("the limits on process {pid} are off"))
}

/// Write a value into a cgroup file.
fn write(path: &Path, value: &str) -> Result<(), String> {
    std::fs::write(path, value).map_err(|error| {
        permission_error(
            path.parent().unwrap_or(Path::new("/")),
            path,
            &format!("cannot write {value:?}: {error}"),
        )
    })
}

/// A message that says what is missing when it is a permission problem.
fn permission_error(root: &Path, path: &Path, detail: &str) -> String {
    let permission = std::io::Error::last_os_error().kind() == std::io::ErrorKind::PermissionDenied;
    if permission {
        format!(
            "{detail} (you do not have permission to use {}; this needs root or a delegated cgroup)",
            root.display()
        )
    } else {
        format!("{detail} ({})", path.display())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root(name: &str) -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("sentinel-cgroup-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn a_limit_is_applied_and_then_released() {
        let root = root("apply");
        let limits = Limits {
            cpu_percent: Some(50.0),
            memory_percent: Some(10.0),
        };

        apply(&root, 42, &limits, 1024 * 1024 * 1024).unwrap();

        let cpu = std::fs::read_to_string(root.join("sentinel-42/cpu.max")).unwrap();
        let memory = std::fs::read_to_string(root.join("sentinel-42/memory.max")).unwrap();
        let procs = std::fs::read_to_string(root.join("sentinel-42/cgroup.procs")).unwrap();
        assert_eq!(cpu, "50000 100000");
        assert_eq!(memory, "107374182");
        assert_eq!(procs, "42");

        release(&root, 42).unwrap();

        assert!(!root.join("sentinel-42").exists());
        assert_eq!(
            std::fs::read_to_string(root.join("cgroup.procs")).unwrap(),
            "42"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_limit_without_any_value_is_refused() {
        let root = root("empty");

        let error = apply(&root, 42, &Limits::default(), 1024).unwrap_err();

        assert!(error.contains("no limit"), "{error}");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_cgroup_that_cannot_be_made_says_so() {
        let root = Path::new("/definitely/not/a/cgroup/root");

        let error = apply(
            root,
            42,
            &Limits {
                cpu_percent: Some(50.0),
                memory_percent: None,
            },
            1024,
        )
        .unwrap_err();

        assert!(error.contains("cannot create the cgroup"), "{error}");
    }
}
