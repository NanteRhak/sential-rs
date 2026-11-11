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
