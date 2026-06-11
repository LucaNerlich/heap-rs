mod common;

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;

#[test]
fn cli_missing_file_exits_with_error() {
    Command::cargo_bin("heap-rs")
        .unwrap()
        .args(["--file", "does-not-exist.hprof", "--quiet"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("error:"));
}

#[test]
fn cli_linked_list_shallow_only() {
    let dir = tempfile::tempdir().unwrap();
    let hprof_path = dir.path().join("list.hprof");
    fs::write(&hprof_path, common::linked_list_hprof()).unwrap();

    Command::cargo_bin("heap-rs")
        .unwrap()
        .args([
            "--file",
            hprof_path.to_str().unwrap(),
            "--shallow-only",
            "--quiet",
            "--top",
            "5",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Shallow Size"))
        .stdout(predicate::str::contains("Done"));
}

#[test]
fn cli_linked_list_full_analysis_with_csv() {
    let dir = tempfile::tempdir().unwrap();
    let hprof_path = dir.path().join("list.hprof");
    let csv_path = dir.path().join("out.csv");
    fs::write(&hprof_path, common::linked_list_hprof()).unwrap();

    Command::cargo_bin("heap-rs")
        .unwrap()
        .args([
            "--file",
            hprof_path.to_str().unwrap(),
            "--quiet",
            "--top",
            "3",
            "--csv",
            csv_path.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Heap Summary"))
        .stdout(predicate::str::contains("Wrote class CSV"));

    let csv = fs::read_to_string(&csv_path).unwrap();
    assert!(csv.contains("com/example/Node"));
}

#[test]
fn cli_holder_fixture_full_analysis() {
    let dir = tempfile::tempdir().unwrap();
    let hprof_path = dir.path().join("holder.hprof");
    fs::write(&hprof_path, common::holder_and_array_hprof()).unwrap();

    Command::cargo_bin("heap-rs")
        .unwrap()
        .args([
            "--file",
            hprof_path.to_str().unwrap(),
            "--quiet",
            "--shallow-only",
            "--top",
            "10",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("int[]"))
        .stdout(predicate::str::contains("Done"));
}
