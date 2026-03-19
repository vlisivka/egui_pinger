use std::io::Write;
use std::process::{Command, Stdio};

fn run_mos(args: &[&str], input: &str) -> (i32, String) {
    let mut child = Command::new("cargo")
        .args(["run", "--"])
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .current_dir("/home/vlisivka/workspace/egui_pinger/mos")
        .spawn()
        .expect("Failed to start mos");

    let mut stdin = child.stdin.take().expect("Failed to open stdin");
    stdin
        .write_all(input.as_bytes())
        .expect("Failed to write to stdin");
    drop(stdin);

    let output = child.wait_with_output().expect("Failed to wait on child");
    let code = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    (code, stdout)
}

#[test]
fn test_cli_help() {
    let (code, stdout) = run_mos(&["--help"], "");
    assert_eq!(code, 0);
    assert!(stdout.contains("Usage: ping <host> | mos [OPTIONS]"));
}

#[test]
fn test_cli_invalid_arg() {
    let child = Command::new("cargo")
        .args(["run", "--", "--invalid-option"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .current_dir("/home/vlisivka/workspace/egui_pinger/mos")
        .spawn()
        .expect("Failed to start mos");

    let output = child.wait_with_output().expect("Failed to wait on child");
    assert_ne!(output.status.code().unwrap_or(0), 0);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Error: Unknown argument"));
}

#[test]
fn test_cli_disable_stat() {
    let input = "64 bytes from 8.8.8.8: time=10.0 ms\n".repeat(5);
    let (code, stdout) = run_mos(&["-n", "5", "--disable", "mos"], &input);
    assert_eq!(code, 0);
    assert!(stdout.contains("# Statistics:"));
    assert!(!stdout.contains("MOS="));
}

#[test]
fn test_empty_input() {
    let (code, stdout) = run_mos(&["-n", "300"], "");
    assert_eq!(code, 0);
    assert!(!stdout.contains("# Statistics:"));
}

#[test]
fn test_all_loss() {
    let input = "Request timeout\n".repeat(3);
    let (code, stdout) = run_mos(&["-n", "3"], &input);
    assert_eq!(code, 0);
    assert!(stdout.contains("L:100.0% (3/3) Av:0% M=0.0ms Med=0.0ms J=0.0ms Jm=0.0ms Jmed=0.0ms MOS=1.0 SD=0.0 Out=0 m/M=inf/-inf 95%=0.0ms Str=3"));
    assert!(stdout.contains("Connectivity lost"));
}

#[test]
fn test_linux_en_stats() {
    let input = std::fs::read_to_string(
        "/home/vlisivka/workspace/egui_pinger/mos/tests/fixtures/linux_en.txt",
    )
    .unwrap();
    let (_, stdout) = run_mos(&["-n", "3"], &input);
    assert!(stdout.contains("# Statistics:"));
    assert!(stdout.contains("M=13.5ms"));
}

#[test]
fn test_linux_ua_long() {
    let input = std::fs::read_to_string(
        "/home/vlisivka/workspace/egui_pinger/mos/tests/fixtures/linux_ua_long.txt",
    )
    .unwrap();
    let (_, stdout) = run_mos(&["-n", "300"], &input);
    assert!(stdout.contains("# Statistics:"));
}
