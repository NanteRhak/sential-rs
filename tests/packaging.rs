//! The packaging: the service unit and the release build.

use std::path::Path;

/// The service unit, as it is installed.
fn unit_path() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("packaging/systemd")
        .join("sentinel.service")
}

/// The sections and keys of an ini file.
fn read_ini(text: &str) -> Vec<(String, String, String)> {
    let mut entries = Vec::new();
    let mut section = String::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            section = name.to_string();
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            entries.push((
                section.clone(),
                key.trim().to_string(),
                value.trim().to_string(),
            ));
        }
    }
    entries
}

fn value(entries: &[(String, String, String)], section: &str, key: &str) -> Option<String> {
    entries
        .iter()
        .find(|(s, k, _)| s == section && k == key)
        .map(|(_, _, value)| value.clone())
}

#[test]
fn the_service_unit_is_valid() {
    let text = std::fs::read_to_string(unit_path()).expect("the service unit is missing");
    let entries = read_ini(&text);

    assert!(!entries.is_empty(), "the service unit is empty");
    for section in ["Unit", "Service", "Install"] {
        assert!(
            entries.iter().any(|(s, _, _)| s == section),
            "the service unit has no [{section}] section"
        );
    }
    for (section, key) in [
        ("Unit", "Description"),
        ("Service", "ExecStart"),
        ("Service", "User"),
        ("Service", "Restart"),
        ("Install", "WantedBy"),
    ] {
        assert!(
            value(&entries, section, key).is_some(),
            "the service unit has no {section}.{key}"
        );
    }
}

#[test]
fn the_service_starts_the_daemon_as_an_unprivileged_user() {
    let text = std::fs::read_to_string(unit_path()).unwrap();
    let entries = read_ini(&text);

    let exec = value(&entries, "Service", "ExecStart").unwrap();
    assert!(exec.contains("sentinel daemon"), "{exec}");

    let user = value(&entries, "Service", "User").unwrap();
    assert_ne!(user, "root", "the service runs as root");
    assert!(!user.is_empty());

    assert_eq!(
        value(&entries, "Service", "Restart").as_deref(),
        Some("on-failure")
    );
}

#[test]
fn the_service_sets_where_the_daemon_logs() {
    let text = std::fs::read_to_string(unit_path()).unwrap();
    let entries = read_ini(&text);

    let exec = value(&entries, "Service", "ExecStart").unwrap();
    assert!(exec.contains("--log-file"), "{exec}");
    assert!(
        value(&entries, "Service", "Environment").is_some(),
        "the service does not say where its configuration is"
    );
}

#[test]
fn the_binary_builds_in_release_mode() {
    let output = std::process::Command::new(std::env::var("CARGO").unwrap_or("cargo".into()))
        .args(["build", "--release", "--quiet"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("failed to run cargo");

    assert!(
        output.status.success(),
        "the release build failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
