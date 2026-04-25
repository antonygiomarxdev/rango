use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

#[test]
fn doctor_passes_on_fresh_workspace() {
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let workspace_path = temp_dir.path();

    // Initialize a fresh workspace
    let mut init_cmd = Command::cargo_bin("rango").expect("failed to get rango binary");
    init_cmd.arg("init").arg(workspace_path).assert().success();

    // Run doctor on the fresh workspace
    let mut doctor_cmd = Command::cargo_bin("rango").expect("failed to get rango binary");
    doctor_cmd
        .arg("doctor")
        .arg(workspace_path)
        .assert()
        .success()
        .stdout(predicate::str::contains("Doctor check complete."))
        .stdout(predicate::str::contains("workspace incompatibility").not());
}

#[test]
fn doctor_fails_on_corrupt_workspace() {
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let workspace_path = temp_dir.path();

    // Initialize a workspace
    let mut init_cmd = Command::cargo_bin("rango").expect("failed to get rango binary");
    init_cmd.arg("init").arg(workspace_path).assert().success();

    // Corrupt the data.redb file by truncating it
    let data_file = workspace_path.join("data.redb");
    fs::write(&data_file, b"garbage").expect("failed to write garbage to data file");

    // Run doctor on the corrupted workspace
    let mut doctor_cmd = Command::cargo_bin("rango").expect("failed to get rango binary");
    doctor_cmd
        .arg("doctor")
        .arg(workspace_path)
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("failed to open engine")
                .or(predicate::str::contains("workspace incompatibility")),
        );
}

#[test]
#[ignore]
fn doctor_fails_on_legacy_record_shape() {
    // TODO: Implement test for legacy record shape detection
    // This would require:
    // 1. Init a workspace
    // 2. Bypass SDK to insert a record missing canonical envelope fields
    // 3. Run doctor and verify it fails with legacy v0.0 shape error
    //
    // For now, this is marked as ignored pending:
    // - Exposure of storage API for direct REDB manipulation
    // - Or a separate utility function to insert malformed records
    // See issue #27 for follow-up
    panic!("not yet implemented");
}
