use std::fs;

use assert_cmd::Command;
use predicates::prelude::predicate;

#[test]
fn binary_with_no_args_prints_usage() {
    Command::cargo_bin("glue")
        .unwrap()
        .assert()
        .failure()
        .stderr(predicate::str::contains("Usage"));
}

#[test]
fn binary_creates_archive_and_extracts_archive() {
    let temp = tempfile::tempdir().unwrap();
    let archive_path = temp.path().join("archive.glue");
    assert!(!archive_path.exists(), "archive file already exists");

    Command::cargo_bin("glue")
        .unwrap()
        .arg("create")
        .args(["-a", archive_path.to_str().unwrap()])
        .args([
            "tests/data/1.txt",
            "tests/data/2.txt",
            "tests/data/data.bin",
        ])
        .assert()
        .success()
        .stderr(predicate::str::is_empty());
    assert!(archive_path.exists(), "archive file was not created");

    Command::cargo_bin("glue")
        .unwrap()
        .current_dir(&temp.path())
        .arg("extract")
        .arg("archive.glue")
        .assert()
        .success()
        .stderr(predicate::str::is_empty());

    let text_file_one_contents = fs::read_to_string("tests/data/1.txt").unwrap();
    let text_file_two_contents = fs::read_to_string("tests/data/2.txt").unwrap();
    let data_file_one_contents = fs::read("tests/data/data.bin").unwrap();

    let extracted_text_file_one_path = temp.path().join("1.txt");
    assert!(
        extracted_text_file_one_path.exists(),
        "file 1.txt is not present after extraction"
    );

    let extracted_text_file_two_path = temp.path().join("2.txt");
    assert!(
        extracted_text_file_two_path.exists(),
        "file 2.txt is not present after extraction"
    );

    let extracted_data_file_one_path = temp.path().join("data.bin");
    assert!(
        extracted_data_file_one_path.exists(),
        "file data.bin is not present after extraction"
    );

    let extracted_text_file_one_contents =
        fs::read_to_string(extracted_text_file_one_path).unwrap();
    let extracted_text_file_two_contents =
        fs::read_to_string(extracted_text_file_two_path).unwrap();
    assert_eq!(text_file_one_contents, extracted_text_file_one_contents);
    assert_eq!(text_file_two_contents, extracted_text_file_two_contents);

    let extracted_data_file_one_contents = fs::read(extracted_data_file_one_path).unwrap();
    assert_eq!(data_file_one_contents, extracted_data_file_one_contents);
}
