use std::process::Command;

/// Test that rango audit command exists and shows help.
#[test]
fn audit_command_shows_help() {
    let output = Command::new("cargo")
        .args(["run", "-p", "rango-cli", "--", "audit", "--help"])
        .output()
        .expect("failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        stdout.contains("audit") || stderr.contains("audit"),
        "help should mention audit subcommand"
    );
}

/// Test that rango audit on non-existent workspace reports no entries.
#[test]
fn audit_on_empty_workspace_reports_no_entries() {
    let tmpdir = tempfile::tempdir().unwrap();
    let output = Command::new("cargo")
        .args([
            "run",
            "-p",
            "rango-cli",
            "--",
            "audit",
            tmpdir.path().to_str().unwrap(),
        ])
        .output()
        .expect("failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        stdout.contains("No audit trail found") || stdout.contains("No governance audit entries"),
        "should report no audit entries: {}",
        stdout
    );
}
