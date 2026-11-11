use std::process::{Command, Output};

fn sentinel(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_sentinel"))
        .args(args)
        .output()
        .expect("failed to run sentinel")
}

#[test]
fn help_lists_the_planned_subcommands() {
    let out = sentinel(&["--help"]);
    assert!(out.status.success(), "exit status: {:?}", out.status);
    let stdout = String::from_utf8_lossy(&out.stdout);

    for cmd in [
        "health", "ps", "top", "tui", "watch", "alerts", "inspect", "kill", "suspend", "resume",
        "renice", "report", "daemon",
    ] {
        assert!(
            stdout
                .lines()
                .any(|line| line.trim_start().starts_with(cmd)),
            "help output is missing `{cmd}`:\n{stdout}"
        );
    }
}

#[test]
fn version_is_printed() {
    let out = sentinel(&["--version"]);
    assert!(out.status.success(), "exit status: {:?}", out.status);
    let stdout = String::from_utf8_lossy(&out.stdout);

    assert!(
        stdout.contains(env!("CARGO_PKG_VERSION")),
        "version output: {stdout}"
    );
}

#[test]
fn health_prints_the_cpu_memory_and_swap_rows() {
    let out = sentinel(&["health", "--interval", "50ms"]);
    assert!(out.status.success(), "exit status: {:?}", out.status);
    let stdout = String::from_utf8_lossy(&out.stdout);

    assert!(stdout.contains("CPU"), "output: {stdout}");
    assert!(stdout.contains("Memory"), "output: {stdout}");
    assert!(stdout.contains("Swap"), "output: {stdout}");
    assert!(stdout.contains("Load average"), "output: {stdout}");
}

#[test]
fn health_prints_the_disk_row() {
    let out = sentinel(&["health", "--interval", "50ms"]);
    assert!(out.status.success(), "exit status: {:?}", out.status);
    let stdout = String::from_utf8_lossy(&out.stdout);

    assert!(stdout.contains("Disk"), "output: {stdout}");
    assert!(stdout.contains("Throughput"), "output: {stdout}");
    assert!(stdout.contains("Filesystem"), "output: {stdout}");
}

#[test]
fn health_prints_the_network_row() {
    let out = sentinel(&["health", "--interval", "50ms"]);
    assert!(out.status.success(), "exit status: {:?}", out.status);
    let stdout = String::from_utf8_lossy(&out.stdout);

    assert!(stdout.contains("Network"), "output: {stdout}");
    assert!(stdout.contains("Packets"), "output: {stdout}");
}

#[test]
fn ps_lists_the_table_header_and_a_running_process() {
    let mut child = Command::new("sleep")
        .arg("30")
        .spawn()
        .expect("failed to start sleep");
    let pid = child.id();

    let out = sentinel(&["ps"]);
    let _ = child.kill();
    let _ = child.wait();

    assert!(out.status.success(), "exit status: {:?}", out.status);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("PID"), "output: {stdout}");
    assert!(stdout.contains("USER"), "output: {stdout}");
    assert!(stdout.contains("MEM%"), "output: {stdout}");
    assert!(stdout.contains("NAME"), "output: {stdout}");
    assert!(
        stdout.contains(&format!("{pid}")),
        "the listing does not mention the running process {pid}:\n{stdout}"
    );
}

#[test]
fn a_bad_interval_is_refused() {
    let out = sentinel(&["health", "--interval", "fast"]);

    assert!(!out.status.success(), "exit status: {:?}", out.status);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("--interval"), "stderr: {stderr}");
}

#[test]
fn broken_local_config_fails_before_running_the_command() {
    let dir = std::env::temp_dir().join(format!("sentinel-cli-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("sentinel.toml"),
        "[general]\ninterval = \"fast\"\n",
    )
    .unwrap();

    let out = Command::new(env!("CARGO_BIN_EXE_sentinel"))
        .arg("health")
        .current_dir(&dir)
        .output()
        .expect("failed to run sentinel");
    std::fs::remove_dir_all(&dir).unwrap();

    assert!(!out.status.success(), "exit status: {:?}", out.status);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("general.interval"), "stderr: {stderr}");
    assert!(stderr.contains("sentinel.toml"), "stderr: {stderr}");
}
