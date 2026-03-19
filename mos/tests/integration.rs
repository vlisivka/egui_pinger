use std::io::Write;
use std::process::{Command, Stdio};

#[test]
fn test_linux_en_stats() {
    let mut child = Command::new("cargo")
        .args(["run", "--", "-n", "3"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .current_dir("/home/vlisivka/workspace/egui_pinger/mos")
        .spawn()
        .expect("Failed to start mos");

    let mut stdin = child.stdin.take().expect("Failed to open stdin");
    let input = std::fs::read_to_string(
        "/home/vlisivka/workspace/egui_pinger/mos/tests/fixtures/linux_en.txt",
    )
    .expect("Failed to read fixture");

    stdin
        .write_all(input.as_bytes())
        .expect("Failed to write to stdin");
    drop(stdin); // Send EOF

    let output = child.wait_with_output().expect("Failed to wait on child");
    let stdout_str = String::from_utf8_lossy(&output.stdout);

    // Should include original output
    assert!(stdout_str.contains("64 bytes from 8.8.8.8: icmp_seq=1 ttl=117 time=13.2 ms"));

    // Should include statistics (should fail now)
    assert!(stdout_str.contains("# Statistics:"));
    assert!(stdout_str.contains("M=13.5ms"));
    assert!(stdout_str.contains("Jmed="));
    assert!(stdout_str.contains("Jm="));
    assert!(stdout_str.contains("L:0.0% (0/3)"));
}

#[test]
fn test_linux_ua_stats() {
    let mut child = Command::new("cargo")
        .args(["run", "--", "-n", "3"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .current_dir("/home/vlisivka/workspace/egui_pinger/mos")
        .spawn()
        .expect("Failed to start mos");

    let mut stdin = child.stdin.take().expect("Failed to open stdin");
    let input = std::fs::read_to_string(
        "/home/vlisivka/workspace/egui_pinger/mos/tests/fixtures/linux_ua.txt",
    )
    .expect("Failed to read fixture");

    stdin
        .write_all(input.as_bytes())
        .expect("Failed to write to stdin");
    drop(stdin);

    let output = child.wait_with_output().expect("Failed to wait on child");
    let stdout_str = String::from_utf8_lossy(&output.stdout);

    assert!(stdout_str.contains("64 байта від 8.8.8.8: icmp_seq=1 ttl=117 час=13.2 мс"));
    assert!(stdout_str.contains("# Statistics:"));
    assert!(stdout_str.contains("M=13.5ms"));
}

#[test]
fn test_windows_ua_stats() {
    let mut child = Command::new("cargo")
        .args(["run", "--", "-n", "3"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .current_dir("/home/vlisivka/workspace/egui_pinger/mos")
        .spawn()
        .expect("Failed to start mos");

    let mut stdin = child.stdin.take().expect("Failed to open stdin");
    let input = std::fs::read_to_string(
        "/home/vlisivka/workspace/egui_pinger/mos/tests/fixtures/windows_ua.txt",
    )
    .expect("Failed to read fixture");

    stdin
        .write_all(input.as_bytes())
        .expect("Failed to write to stdin");
    drop(stdin);

    let output = child.wait_with_output().expect("Failed to wait on child");
    let stdout_str = String::from_utf8_lossy(&output.stdout);

    assert!(stdout_str.contains("Відповідь від 8.8.8.8: число байтів=32 час=13мс TTL=117"));
    assert!(stdout_str.contains("# Statistics:"));
    assert!(stdout_str.contains("M=13.0ms")); // (13 + 14 + 12) / 3 = 39 / 3 = 13.0
}

#[test]
fn test_incident_detection() {
    let mut child = Command::new("cargo")
        .args(["run", "--", "-n", "999"]) // don't print stats automatically
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .current_dir("/home/vlisivka/workspace/egui_pinger/mos")
        .spawn()
        .expect("Failed to start mos");

    let mut stdin = child.stdin.take().expect("Failed to open stdin");
    // Fixture with some timeouts to trigger incident
    let input = "64 bytes from 8.8.8.8: icmp_seq=1 ttl=117 time=13 ms
Request timeout for icmp_seq 2
Request timeout for icmp_seq 3
Request timeout for icmp_seq 4
64 bytes from 8.8.8.8: icmp_seq=5 ttl=117 time=14 ms
";

    stdin
        .write_all(input.as_bytes())
        .expect("Failed to write to stdin");
    drop(stdin);

    let output = child.wait_with_output().expect("Failed to wait on child");
    let stdout_str = String::from_utf8_lossy(&output.stdout);

    assert!(stdout_str.contains("Connectivity lost"));
    assert!(stdout_str.contains("Connectivity restored"));
}

#[test]
fn test_linux_ua_long() {
    let mut child = Command::new("cargo")
        .args(["run", "--", "-n", "300"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .current_dir("/home/vlisivka/workspace/egui_pinger/mos")
        .spawn()
        .expect("Failed to start mos");

    let mut stdin = child.stdin.take().expect("Failed to open stdin");
    let input = std::fs::read_to_string(
        "/home/vlisivka/workspace/egui_pinger/mos/tests/fixtures/linux_ua_long.txt",
    )
    .expect("Failed to read fixture");

    stdin
        .write_all(input.as_bytes())
        .expect("Failed to write to stdin");
    drop(stdin);

    let output = child.wait_with_output().expect("Failed to wait on child");
    let stdout_str = String::from_utf8_lossy(&output.stdout);

    // After 300 lines we should have statistics
    assert!(stdout_str.contains("# Statistics:"));
}
