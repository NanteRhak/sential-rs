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
fn ps_json_output_parses() {
    let out = sentinel(&["ps", "--json"]);
    assert!(out.status.success(), "exit status: {:?}", out.status);
    let stdout = String::from_utf8_lossy(&out.stdout);

    let value: serde_json::Value =
        serde_json::from_str(&stdout).expect("ps --json is not valid JSON");
    let rows = value["processes"].as_array().expect("no processes array");
    assert!(!rows.is_empty(), "the process list is empty");
    assert!(rows[0]["pid"].is_number(), "{stdout}");
    assert!(rows[0]["name"].is_string(), "{stdout}");
}

#[test]
fn top_prints_the_process_table_sorted_by_cpu() {
    let out = sentinel(&["top"]);
    assert!(out.status.success(), "exit status: {:?}", out.status);
    let stdout = String::from_utf8_lossy(&out.stdout);

    assert!(stdout.contains("CPU%"), "output: {stdout}");
}

#[test]
fn inspect_shows_the_detail_of_a_process() {
    let mut child = Command::new("sleep").arg("30").spawn().unwrap();
    let pid = child.id();

    let out = sentinel(&["inspect", &pid.to_string()]);
    let _ = child.kill();
    let _ = child.wait();

    assert!(out.status.success(), "exit status: {:?}", out.status);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Command"), "output: {stdout}");
    assert!(stdout.contains("sleep"), "output: {stdout}");
    assert!(stdout.contains("Cgroup"), "output: {stdout}");
}

#[test]
fn inspecting_a_process_that_is_not_there_fails() {
    let out = sentinel(&["inspect", "999999999"]);

    assert!(!out.status.success(), "exit status: {:?}", out.status);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("999999999"), "stderr: {stderr}");
}

#[test]
fn an_alert_is_printed_as_it_is_raised() {
    let dir = std::env::temp_dir().join(format!("sentinel-alert-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("sentinel.toml"),
        "[[alerts.rules]]\nname = \"always\"\nmetric = \"load_avg_1\"\ncondition = \"above\"\nthreshold = -1.0\nduration = \"1ms\"\nseverity = \"info\"\nmessage = \"the load average is above -1\"\n",
    )
    .unwrap();

    let out = Command::new(env!("CARGO_BIN_EXE_sentinel"))
        .args(["health", "--interval", "50ms"])
        .current_dir(&dir)
        .output()
        .expect("failed to run sentinel");
    std::fs::remove_dir_all(&dir).unwrap();

    assert!(out.status.success(), "exit status: {:?}", out.status);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("[INFO]"), "stdout: {stdout}");
    assert!(
        stdout.contains("the load average is above -1"),
        "stdout: {stdout}"
    );
}

#[test]
fn alerts_are_read_back_from_the_log_file() {
    let dir = std::env::temp_dir().join(format!("sentinel-log-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("sentinel.toml"),
        "[alerts]\nlog_file = \"alerts.jsonl\"\n\n[[alerts.rules]]\nname = \"always\"\nmetric = \"load_avg_1\"\ncondition = \"above\"\nthreshold = -1.0\nduration = \"1ms\"\nseverity = \"warning\"\nmessage = \"the load average is above -1\"\n",
    )
    .unwrap();

    let health = Command::new(env!("CARGO_BIN_EXE_sentinel"))
        .args(["health", "--interval", "50ms"])
        .current_dir(&dir)
        .output()
        .expect("failed to run sentinel");
    assert!(health.status.success(), "exit status: {:?}", health.status);

    let out = Command::new(env!("CARGO_BIN_EXE_sentinel"))
        .args(["alerts", "--last", "1h"])
        .current_dir(&dir)
        .output()
        .expect("failed to run sentinel");

    let log = std::fs::read_to_string(dir.join("alerts.jsonl")).unwrap_or_default();
    std::fs::remove_dir_all(&dir).unwrap();

    assert!(out.status.success(), "exit status: {:?}", out.status);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("[WARNING]"), "stdout: {stdout}");
    assert!(
        stdout.contains("the load average is above -1"),
        "stdout: {stdout}"
    );
    assert!(log.contains("the load average is above -1"), "log: {log}");
}

#[test]
fn alerts_without_a_log_file_says_so() {
    let out = sentinel(&["alerts"]);

    assert!(!out.status.success(), "exit status: {:?}", out.status);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("no alert log"), "stderr: {stderr}");
}

fn sleeping_child() -> std::process::Child {
    Command::new("sleep")
        .arg("30")
        .spawn()
        .expect("failed to start sleep")
}

#[test]
fn kill_dry_run_says_what_it_would_do_and_leaves_the_process_alone() {
    let mut child = sleeping_child();
    let pid = child.id();

    let out = sentinel(&["kill", &pid.to_string(), "--dry-run"]);
    let still_running = child.try_wait().unwrap().is_none();
    let _ = child.kill();
    let _ = child.wait();

    assert!(out.status.success(), "exit status: {:?}", out.status);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("would kill"), "stdout: {stdout}");
    assert!(still_running, "the dry run killed the process");
}

#[test]
fn kill_terminates_the_process() {
    let mut child = sleeping_child();
    let pid = child.id();

    let out = sentinel(&["kill", &pid.to_string(), "--yes"]);
    let _ = child.wait();

    assert!(out.status.success(), "exit status: {:?}", out.status);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("SIGTERM"), "stdout: {stdout}");
    assert!(
        !child.try_wait().unwrap().is_none(),
        "the process is still running"
    );
}

#[test]
fn kill_refuses_a_protected_process_and_names_the_rule() {
    let dir = std::env::temp_dir().join(format!("sentinel-protect-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("sentinel.toml"),
        "[critical_processes]\nnames = [\"sleep\"]\n",
    )
    .unwrap();
    let mut child = sleeping_child();
    let pid = child.id();

    let out = Command::new(env!("CARGO_BIN_EXE_sentinel"))
        .args(["kill", &pid.to_string(), "--yes"])
        .current_dir(&dir)
        .output()
        .expect("failed to run sentinel");
    let still_running = child.try_wait().unwrap().is_none();
    let _ = child.kill();
    let _ = child.wait();
    std::fs::remove_dir_all(&dir).unwrap();

    assert!(!out.status.success(), "exit status: {:?}", out.status);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("protected"), "stderr: {stderr}");
    assert!(stderr.contains("critical_processes"), "stderr: {stderr}");
    assert!(still_running, "a protected process was killed");
}

#[test]
fn kill_writes_to_the_action_log() {
    let dir = std::env::temp_dir().join(format!("sentinel-action-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("sentinel.toml"),
        "[actions]\nlog_file = \"actions.jsonl\"\n",
    )
    .unwrap();
    let mut child = sleeping_child();
    let pid = child.id();

    let out = Command::new(env!("CARGO_BIN_EXE_sentinel"))
        .args(["kill", &pid.to_string(), "--dry-run"])
        .current_dir(&dir)
        .output()
        .expect("failed to run sentinel");
    let _ = child.kill();
    let _ = child.wait();
    let log = std::fs::read_to_string(dir.join("actions.jsonl")).unwrap_or_default();
    std::fs::remove_dir_all(&dir).unwrap();

    assert!(out.status.success(), "exit status: {:?}", out.status);
    assert!(log.contains("action"), "log: {log}");
    assert!(log.contains("kill"), "log: {log}");
    assert!(log.contains(&pid.to_string()), "log: {log}");
    assert!(log.contains("user_requested"), "log: {log}");
}

#[test]
fn suspend_stops_the_process_and_resume_starts_it_again() {
    let mut child = sleeping_child();
    let pid = child.id();

    let out = sentinel(&["suspend", &pid.to_string(), "--yes"]);
    assert!(out.status.success(), "exit status: {:?}", out.status);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("SIGSTOP"), "stdout: {stdout}");

    let state = std::fs::read_to_string(format!("/proc/{pid}/stat")).unwrap();
    assert!(state.contains(" T "), "the process is not stopped: {state}");

    let out = sentinel(&["resume", &pid.to_string(), "--yes"]);
    assert!(out.status.success(), "exit status: {:?}", out.status);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("SIGCONT"), "stdout: {stdout}");

    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn renice_changes_the_priority_of_a_process() {
    let mut child = sleeping_child();
    let pid = child.id();

    let out = sentinel(&["renice", &pid.to_string(), "5", "--yes"]);
    let state = std::fs::read_to_string(format!("/proc/{pid}/stat")).unwrap();
    let _ = child.kill();
    let _ = child.wait();

    assert!(out.status.success(), "exit status: {:?}", out.status);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("reniced"), "stdout: {stdout}");
    // Field 19 of /proc/<pid>/stat is the nice value.
    let fields: Vec<&str> = state
        .rsplit(')')
        .next()
        .unwrap()
        .split_whitespace()
        .collect();
    assert_eq!(fields[16], "5", "the nice value did not change: {state}");
}

#[test]
fn an_unknown_signal_is_refused() {
    let out = sentinel(&["kill", "1", "--signal", "SIGBOGUS", "--yes"]);

    assert!(!out.status.success(), "exit status: {:?}", out.status);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("--signal"), "stderr: {stderr}");
}

#[test]
fn killing_a_process_that_is_not_there_fails() {
    let out = sentinel(&["kill", "999999999", "--yes"]);

    assert!(!out.status.success(), "exit status: {:?}", out.status);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("999999999"), "stderr: {stderr}");
}

#[test]
fn watch_samples_at_the_requested_rate_and_stops_itself() {
    let out = sentinel(&["watch", "--interval", "100ms", "--duration", "300ms"]);

    assert!(out.status.success(), "exit status: {:?}", out.status);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("sampling every 100ms"), "stdout: {stdout}");
    let lines = stdout.lines().filter(|line| line.contains("cpu")).count();
    assert!(lines >= 2, "only {lines} samples in 300ms:\n{stdout}");
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
