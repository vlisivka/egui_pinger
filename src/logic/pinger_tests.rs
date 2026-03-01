use super::*;
use crate::model::DisplaySettings;
use std::collections::HashSet;

fn test_host(mode: PingMode, packet_size: usize, random_padding: bool) -> HostInfo {
    HostInfo {
        name: "Test".to_string(),
        address: "1.2.3.4".to_string(),
        mode,
        display: DisplaySettings::default(),
        packet_size,
        random_padding,
    }
}

// --- Interval jitter tests ---

#[test]
fn test_jitter_produces_non_constant_intervals() {
    // For each mode, generate 100 intervals and verify they are not all identical
    let modes = [
        PingMode::VeryFast,
        PingMode::Fast,
        PingMode::NotFast,
        PingMode::Normal,
        PingMode::NotSlow,
        PingMode::Slow,
        PingMode::VerySlow,
    ];
    let mut rng = rand::rng();
    for mode in modes {
        let intervals: Vec<Duration> = (0..100).map(|_| compute_interval(mode, &mut rng)).collect();
        let unique: HashSet<u128> = intervals.iter().map(|d| d.as_nanos()).collect();
        assert!(
            unique.len() > 1,
            "Mode {:?} produced identical intervals — jitter is broken",
            mode
        );
    }
}

#[test]
fn test_jitter_stays_within_bounds() {
    let cases: Vec<(PingMode, f64, f64)> = vec![
        (PingMode::VeryFast, 0.95, 1.05),
        (PingMode::Fast, 1.8, 2.2),
        (PingMode::NotFast, 4.5, 5.5),
        (PingMode::Normal, 9.0, 11.0),
        (PingMode::NotSlow, 27.0, 33.0),
        (PingMode::Slow, 55.0, 65.0),
        (PingMode::VerySlow, 285.0, 315.0),
    ];
    let mut rng = rand::rng();
    for (mode, min_s, max_s) in cases {
        for _ in 0..200 {
            let d = compute_interval(mode, &mut rng);
            let secs = d.as_secs_f64();
            assert!(
                secs >= min_s && secs <= max_s,
                "Mode {:?}: interval {:.4}s outside [{:.2}, {:.2}]",
                mode,
                secs,
                min_s,
                max_s
            );
        }
    }
}

#[test]
fn test_jitter_has_sufficient_entropy() {
    // Statistical check: standard deviation of 1000 intervals should be > 0
    let mut rng = rand::rng();
    let intervals: Vec<f64> = (0..1000)
        .map(|_| compute_interval(PingMode::Normal, &mut rng).as_secs_f64())
        .collect();
    let mean = intervals.iter().sum::<f64>() / intervals.len() as f64;
    let variance =
        intervals.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / intervals.len() as f64;
    let stddev = variance.sqrt();
    // For Normal mode (10s ± 1s), stddev should be roughly 0.5-0.6s for uniform dist
    assert!(
        stddev > 0.1,
        "Jitter stddev {:.4} is too low — intervals are too uniform",
        stddev
    );
}

// --- Payload tests ---

#[test]
fn test_payload_respects_configured_size() {
    let host = test_host(PingMode::Fast, 64, false);
    for _ in 0..50 {
        let payload = generate_payload(&host);
        assert_eq!(
            payload.len(),
            64,
            "Payload length should match configured size"
        );
    }
}

#[test]
fn test_payload_size_is_clamped() {
    // Too small
    let host_small = test_host(PingMode::Fast, 4, false);
    let payload = generate_payload(&host_small);
    assert_eq!(payload.len(), 16, "Size below 16 should be clamped to 16");

    // Too large
    let host_large = test_host(PingMode::Fast, 9999, false);
    let payload = generate_payload(&host_large);
    assert_eq!(
        payload.len(),
        1400,
        "Size above 1400 should be clamped to 1400"
    );
}

#[test]
fn test_random_padding_varies_size() {
    let host = test_host(PingMode::Fast, 100, true);
    let sizes: Vec<usize> = (0..100).map(|_| generate_payload(&host).len()).collect();
    let unique: HashSet<usize> = sizes.into_iter().collect();
    // With 0-25% padding on 100 bytes, sizes range from 100 to 125
    assert!(
        unique.len() > 1,
        "Random padding should produce varying payload sizes"
    );
    for &s in &unique {
        assert!(s >= 100 && s <= 125, "Padded size {} outside [100, 125]", s);
    }
}

#[test]
fn test_payload_content_is_random() {
    let host = test_host(PingMode::Fast, 64, false);
    let p1 = generate_payload(&host);
    let p2 = generate_payload(&host);
    // Two truly random payloads of 64 bytes should differ
    assert_ne!(p1, p2, "Consecutive payloads should not be identical");
}

#[test]
fn test_payload_bytes_not_constant() {
    // Verify that all bytes are not the same value (not filled with e.g. 42 or 0)
    let host = test_host(PingMode::Fast, 256, false);
    let payload = generate_payload(&host);
    let unique_bytes: HashSet<u8> = payload.iter().copied().collect();
    // 256 random bytes should have many distinct values
    assert!(
        unique_bytes.len() > 10,
        "Only {} distinct byte values in 256-byte payload — not random enough",
        unique_bytes.len()
    );
}

#[test]
fn test_no_padding_keeps_exact_size() {
    let host = test_host(PingMode::Fast, 200, false);
    for _ in 0..100 {
        let payload = generate_payload(&host);
        assert_eq!(
            payload.len(),
            200,
            "Without random_padding, size must be exactly the configured value"
        );
    }
}

#[tokio::test]
async fn test_ipv6_parsing() {
    let address = "2001:4860:4860::8888";
    let ip = address.parse::<IpAddr>();
    assert!(ip.is_ok());
    assert!(ip.unwrap().is_ipv6());
}

#[tokio::test]
async fn test_ipv6_hostname_resolution() {
    // google.com usually has AAAA records
    let address = "google.com";
    let lookup_str = format!("{}:0", address);
    let addrs = tokio::net::lookup_host(&lookup_str).await;
    assert!(addrs.is_ok());
    let mut addrs = addrs.unwrap();
    // Verify we can find at least one IP
    assert!(addrs.next().is_some());
}

#[tokio::test]
async fn test_ipv6_bracketed_resolution() {
    // This is where it might fail if we don't bracket IPv6
    // address.parse::<IpAddr>() fails for "[::1]"
    let address = "[::1]";
    
    let clean_address = if address.starts_with('[') && address.ends_with(']') {
        &address[1..address.len() - 1]
    } else {
        &address
    };
    
    let ip = clean_address.parse::<IpAddr>();
    assert!(ip.is_ok());
    assert_eq!(ip.unwrap(), "::1".parse::<IpAddr>().unwrap());
    
    // Test the fallback logic in pinger_task
    let lookup_str = format!("{}:0", address);
    let res = tokio::net::lookup_host(&lookup_str).await;
    assert!(res.is_ok(), "lookup_host should handle bracketed [::1]:0");
}

#[tokio::test]
async fn test_ipv6_long_address_parsing() {
    // Full IPv6 address (39 characters)
    let address = "2001:0db8:85a3:0000:0000:8a2e:0370:7334";
    assert!(address.parse::<IpAddr>().is_ok());
    
    // Bracketed short IPv6
    let address2 = "[2001:db8::1]";
    let clean = if address2.starts_with('[') && address2.ends_with(']') {
        &address2[1..address2.len() - 1]
    } else {
        &address2
    };
    assert!(clean.parse::<IpAddr>().is_ok());
}
