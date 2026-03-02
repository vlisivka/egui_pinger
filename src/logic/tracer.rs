use regex::Regex;
use std::time::Duration;
use tokio::process::Command as TokioCommand;

#[derive(Debug, Clone, Default)]
pub struct TracerouteHop {
    pub hop_number: u8,
    pub address: Option<String>,
    pub rtt: Option<Duration>,
}

/// Executes a system traceroute command and parses the output for IP addresses.
/// The process is killed after 30 seconds to avoid hanging on network outages.
pub async fn run_traceroute(address: &str) -> Vec<String> {
    let child = if cfg!(windows) {
        // Windows: tracert -d -h 20 -w 2000 <address>
        match TokioCommand::new("tracert")
            .args(["-d", "-h", "20", "-w", "2000", address])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Failed to spawn tracert: {}", e);
                return Vec::new();
            }
        }
    } else {
        // Linux/macOS: traceroute -n -m 20 -q 1 -w 2 <address>
        match TokioCommand::new("traceroute")
            .args(["-n", "-m", "20", "-q", "1", "-w", "2", address])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Failed to spawn traceroute: {}", e);
                return Vec::new();
            }
        }
    };

    // Kill the process if it runs longer than 30 seconds
    let result = tokio::time::timeout(Duration::from_secs(30), child.wait_with_output()).await;
    match result {
        Ok(Ok(output)) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            parse_traceroute_output(&stdout)
        }
        Ok(Err(e)) => {
            eprintln!("Traceroute process error: {}", e);
            Vec::new()
        }
        Err(_) => {
            eprintln!("Traceroute timed out after 30s for {}", address);
            // child is consumed by wait_with_output, process will be killed on drop
            Vec::new()
        }
    }
}

/// Parses the output of traceroute/tracert to extract IP addresses.
pub fn parse_traceroute_output(output: &str) -> Vec<String> {
    let mut ips = Vec::new();
    // Regex to match IPv4 or IPv6 addresses.
    // Basic pattern that matches typical IP representations.
    let ip_re =
        Regex::new(r"(\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3})|([0-9a-fA-F:]+:[0-9a-fA-F:]+)").unwrap();

    for line in output.lines() {
        // Skip header lines - usually start with "traceroute" or "Tracing"
        if line.starts_with("traceroute") || line.starts_with("Tracing") || line.is_empty() {
            continue;
        }

        // Windows output might have "*" for timeouts.
        // We only care about lines with actual IPs.
        if let Some(caps) = ip_re.captures(line) {
            if let Some(m) = caps.get(0) {
                let ip = m.as_str().to_string();
                // Avoid empty strings or just colons
                if ip.len() > 3 && !ips.contains(&ip) {
                    ips.push(ip);
                }
            }
        }
    }
    ips
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_linux_traceroute() {
        let output = r#"traceroute to 8.8.8.8 (8.8.8.8), 30 hops max, 60 byte packets
 1  192.168.1.1  0.501 ms  0.463 ms  0.439 ms
 2  10.0.0.1  2.456 ms  2.321 ms  2.210 ms
 3  * * *
 4  72.14.232.1  12.345 ms
 5  8.8.8.8  14.567 ms"#;
        let ips = parse_traceroute_output(output);
        assert_eq!(
            ips,
            vec!["192.168.1.1", "10.0.0.1", "72.14.232.1", "8.8.8.8"]
        );
    }

    #[test]
    fn test_parse_windows_tracert() {
        let output = r#"Tracing route to dns.google [8.8.8.8]
over a maximum of 30 hops:

  1    <1 ms    <1 ms    <1 ms  192.168.1.1 
  2     2 ms     2 ms     2 ms  10.0.0.1 
  3     *        *        *     Request timed out.
  4    12 ms    13 ms    12 ms  72.14.232.1 
  5    14 ms    14 ms    14 ms  8.8.8.8 

Trace complete."#;
        let ips = parse_traceroute_output(output);
        assert_eq!(
            ips,
            vec!["192.168.1.1", "10.0.0.1", "72.14.232.1", "8.8.8.8"]
        );
    }

    #[test]
    fn test_parse_ipv6_traceroute() {
        let output = r#"traceroute to 2001:4860:4860::8888 (2001:4860:4860::8888), 30 hops max
 1  2001:db8::1  0.5 ms
 2  2001:db8::2  1.2 ms
 3  2001:4860:4860::8888  5.6 ms"#;
        let ips = parse_traceroute_output(output);
        assert_eq!(
            ips,
            vec!["2001:db8::1", "2001:db8::2", "2001:4860:4860::8888"]
        );
    }
}
