use super::*;
use crate::model::{DisplaySettings, HostStatus};
use std::collections::HashSet;

fn test_host(mode: PingMode, packet_size: usize, random_padding: bool) -> HostInfo {
    HostInfo {
        name: "Test".to_string(),
        address: "1.2.3.4".to_string(),
        mode,
        display: DisplaySettings::default(),
        packet_size,
        random_padding,
        log_to_file: false,
        log_file_path: String::new(),
        is_stopped: false,
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
        let payload = generate_payload(Some(&host));
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
    let payload = generate_payload(Some(&host_small));
    assert_eq!(payload.len(), 16, "Size below 16 should be clamped to 16");

    // Too large
    let host_large = test_host(PingMode::Fast, 9999, false);
    let payload = generate_payload(Some(&host_large));
    assert_eq!(
        payload.len(),
        1400,
        "Size above 1400 should be clamped to 1400"
    );
}

#[test]
fn test_random_padding_varies_size() {
    let host = test_host(PingMode::Fast, 100, true);
    let sizes: Vec<usize> = (0..100)
        .map(|_| generate_payload(Some(&host)).len())
        .collect();
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
    let p1 = generate_payload(Some(&host));
    let p2 = generate_payload(Some(&host));
    // Two truly random payloads of 64 bytes should differ
    assert_ne!(p1, p2, "Consecutive payloads should not be identical");
}

#[test]
fn test_payload_bytes_not_constant() {
    // Verify that all bytes are not the same value (not filled with e.g. 42 or 0)
    let host = test_host(PingMode::Fast, 256, false);
    let payload = generate_payload(Some(&host));
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
        let payload = generate_payload(Some(&host));
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

// --- Failure deduction tests ---

#[test]
fn test_failure_deduction_host_down_hops_up() {
    let state = Arc::new(Mutex::new(AppState::default()));
    let target_addr = "8.8.8.8".to_string();
    let hop1 = "192.168.1.1".to_string();

    {
        let mut sl = state.lock().unwrap();
        sl.hosts.push(test_host(PingMode::Fast, 16, false));
        sl.hosts[0].address = target_addr.clone();

        let mut status = HostStatus::default();
        status.streak_success = false;
        status.streak = 3; // officially down
        status.traceroute_path = vec![hop1.clone(), target_addr.clone()];
        sl.statuses.insert(target_addr.clone(), status);

        // Hop 1 is alive
        let mut h1_status = HostStatus::default();
        h1_status.streak_success = true;
        h1_status.streak = 10;
        h1_status.last_updated = Some(Instant::now());
        sl.statuses.insert(hop1.clone(), h1_status);
    }

    deduce_failure_points(&state, Instant::now());

    let sl = state.lock().unwrap();
    let status = sl.statuses.get(&target_addr).unwrap();
    assert_eq!(
        status.failure_point, None,
        "Problem on host itself, failure_point should be None"
    );
}

#[test]
fn test_failure_deduction_hop_down() {
    let state = Arc::new(Mutex::new(AppState::default()));
    let target_addr = "8.8.8.8".to_string();
    let hop1 = "192.168.1.1".to_string();

    {
        let mut sl = state.lock().unwrap();
        sl.hosts.push(test_host(PingMode::Fast, 16, false));
        sl.hosts[0].address = target_addr.clone();

        let mut status = HostStatus::default();
        status.streak_success = false;
        status.streak = 3;
        status.traceroute_path = vec![hop1.clone(), target_addr.clone()];
        sl.statuses.insert(target_addr.clone(), status);

        // Hop 1 is broken
        let mut h1_status = HostStatus::default();
        h1_status.streak_success = false;
        h1_status.streak = 3;
        h1_status.last_updated = Some(Instant::now());
        sl.statuses.insert(hop1.clone(), h1_status);
    }

    deduce_failure_points(&state, Instant::now());

    let sl = state.lock().unwrap();
    let status = sl.statuses.get(&target_addr).unwrap();
    assert_eq!(
        status.failure_point,
        Some("Local Interface".to_string()),
        "Hop 1 is local and broken, should be reported as Local Interface"
    );
}

#[test]
fn test_failure_deduction_remote_hop_down() {
    let state = Arc::new(Mutex::new(AppState::default()));
    let target_addr = "8.8.8.8".to_string();
    let hop1 = "192.168.1.1".to_string();
    let hop2 = "9.9.9.9".to_string();

    {
        let mut sl = state.lock().unwrap();
        sl.hosts.push(test_host(PingMode::Fast, 16, false));
        sl.hosts[0].address = target_addr.clone();

        let mut status = HostStatus::default();
        status.streak_success = false;
        status.streak = 3;
        status.traceroute_path = vec![hop1.clone(), hop2.clone(), target_addr.clone()];
        sl.statuses.insert(target_addr.clone(), status);

        // Hop 1 is alive
        let mut h1_status = HostStatus::default();
        h1_status.streak_success = true;
        h1_status.streak = 10;
        h1_status.last_updated = Some(Instant::now());
        sl.statuses.insert(hop1.clone(), h1_status);

        // Hop 2 is broken
        let mut h2_status = HostStatus::default();
        h2_status.streak_success = false;
        h2_status.streak = 3;
        h2_status.last_updated = Some(Instant::now());
        sl.statuses.insert(hop2.clone(), h2_status);
    }

    deduce_failure_points(&state, Instant::now());

    let sl = state.lock().unwrap();
    let status = sl.statuses.get(&target_addr).unwrap();
    assert_eq!(
        status.failure_point,
        Some(hop2),
        "Hop 2 (remote) is broken, should be the failure point"
    );
}

#[test]
fn test_failure_deduction_local_breakdown() {
    let state = Arc::new(Mutex::new(AppState::default()));
    let target_addr = "8.8.8.8".to_string();

    {
        let mut sl = state.lock().unwrap();
        sl.hosts.push(test_host(PingMode::Fast, 16, false));
        sl.hosts[0].address = target_addr.clone();

        // Path only contains target
        let mut status = HostStatus::default();
        status.streak_success = false;
        status.streak = 3;
        status.traceroute_path = vec![target_addr.clone()];
        sl.statuses.insert(target_addr.clone(), status);
    }

    deduce_failure_points(&state, Instant::now());

    let sl = state.lock().unwrap();
    let status = sl.statuses.get(&target_addr).unwrap();
    assert_eq!(status.failure_point, Some("Local Interface".to_string()));
}

#[test]
fn test_failure_deduction_gateway_down() {
    let state = Arc::new(Mutex::new(AppState::default()));
    let target_addr = "8.8.8.8".to_string();
    let hop1 = "10.0.0.1".to_string(); // gateway

    {
        let mut sl = state.lock().unwrap();
        sl.hosts.push(test_host(PingMode::Fast, 16, false));
        sl.hosts[0].address = target_addr.clone();

        let mut status = HostStatus::default();
        status.streak_success = false;
        status.streak = 3;
        status.traceroute_path = vec![hop1.clone(), "2.2.2.2".to_string(), target_addr.clone()];
        sl.statuses.insert(target_addr.clone(), status);

        // Gateway is broken
        let mut h1_status = HostStatus::default();
        h1_status.streak_success = false;
        h1_status.streak = 3;
        h1_status.last_updated = Some(Instant::now());
        sl.statuses.insert(hop1.clone(), h1_status);
    }

    deduce_failure_points(&state, Instant::now());

    let sl = state.lock().unwrap();
    let status = sl.statuses.get(&target_addr).unwrap();
    assert_eq!(status.failure_point, Some("Local Interface".to_string()));
}

#[test]
fn test_failure_deduction_stale_data() {
    let state = Arc::new(Mutex::new(AppState::default()));
    let target_addr = "8.8.8.8".to_string();
    let hop1 = "192.168.1.1".to_string();

    {
        let mut sl = state.lock().unwrap();
        sl.hosts.push(test_host(PingMode::Fast, 16, false));
        sl.hosts[0].address = target_addr.clone();

        let mut status = HostStatus::default();
        status.streak_success = false;
        status.streak = 3;
        status.traceroute_path = vec![hop1.clone(), target_addr.clone()];
        sl.statuses.insert(target_addr.clone(), status);

        // Hop 1 has stale data
        let mut h1_status = HostStatus::default();
        h1_status.streak_success = true;
        h1_status.streak = 10;
        h1_status.last_updated = Some(Instant::now() - Duration::from_secs(3600));
        sl.statuses.insert(hop1.clone(), h1_status);
    }

    deduce_failure_points(&state, Instant::now());

    let sl = state.lock().unwrap();
    let status = sl.statuses.get(&target_addr).unwrap();
    assert_eq!(
        status.failure_point,
        Some("Local Interface".to_string()),
        "Hop 1 is local (stale), so it counts as Local Interface failure"
    );
}

// --- Incident detection tests ---

#[tokio::test]
async fn test_incident_detection_timeout_streak() {
    let state = Arc::new(Mutex::new(AppState::default()));
    let address = "1.2.3.4".to_string();

    {
        let mut sl = state.lock().unwrap();
        sl.statuses.insert(address.clone(), HostStatus::default());
    }

    // Simulate 3 timeouts
    for _ in 0..STATE_CONFIRMATION_STREAK {
        process_ping_result(&state, &address, false, f64::NAN, None);
    }

    let sl = state.lock().unwrap();
    let status = sl.statuses.get(&address).unwrap();

    // Check events
    let incidents: Vec<_> = status
        .events
        .iter()
        .filter(|e| matches!(e, LogEntry::Incident { .. }))
        .collect();
    assert_eq!(incidents.len(), 1, "Should have one incident");
    if let Some(LogEntry::Incident { is_break, .. }) = incidents.get(0) {
        assert!(is_break, "Incident should be a break");
    }
}

#[tokio::test]
async fn test_incident_restoration() {
    let state = Arc::new(Mutex::new(AppState::default()));
    let address = "1.2.3.4".to_string();

    {
        let mut sl = state.lock().unwrap();
        sl.statuses.insert(address.clone(), HostStatus::default());
    }

    // Streak failures
    for _ in 0..STATE_CONFIRMATION_STREAK {
        process_ping_result(&state, &address, false, f64::NAN, None);
    }

    // Now one success
    process_ping_result(&state, &address, true, 10.0, None);

    let sl = state.lock().unwrap();
    let status = sl.statuses.get(&address).unwrap();

    let incidents: Vec<_> = status
        .events
        .iter()
        .filter(|e| matches!(e, LogEntry::Incident { .. }))
        .collect();
    assert_eq!(
        incidents.len(),
        2,
        "Should have two incidents: break and restoration"
    );

    if let Some(LogEntry::Incident {
        is_break,
        downtime_sec,
        ..
    }) = incidents.get(1)
    {
        assert!(!is_break, "Incident should be a restoration");
        assert!(downtime_sec.is_some(), "Downtime should be recorded");
    }
}

// --- Traceroute update tests ---

#[test]
fn test_traceroute_path_update_logic() {
    let target = "8.8.8.8";
    let old_full = vec!["1.1.1.1".to_string(), target.to_string()];
    let new_incomplete = vec!["1.1.1.1".to_string()];
    let new_empty: Vec<String> = vec![];

    // 1. Empty doesn't overwrite
    assert!(!should_update_traceroute_path(
        &old_full, &new_empty, true, target
    ));

    // 2. Incomplete doesn't overwrite full
    assert!(!should_update_traceroute_path(
        &old_full,
        &new_incomplete,
        true,
        target
    ));

    // 3. New full replaces old full (equal or longer)
    assert!(should_update_traceroute_path(
        &old_full, &old_full, true, target
    ));

    // 4. Full replaces incomplete
    assert!(should_update_traceroute_path(
        &new_incomplete,
        &old_full,
        true,
        target
    ));

    // 5. Short incomplete doesn't replace long incomplete
    let long_incomplete = vec!["1.1.1.1".to_string(), "2.2.2.2".to_string()];
    assert!(!should_update_traceroute_path(
        &long_incomplete,
        &new_incomplete,
        true,
        target
    ));
}

#[tokio::test]
async fn test_traceroute_spawn_logic() {
    let state = Arc::new(Mutex::new(AppState::default()));
    let address = "1.2.3.4".to_string();
    let mut last_trace_times = HashMap::new();
    let now = Instant::now();

    {
        let mut sl = state.lock().unwrap();
        sl.hosts.push(test_host(PingMode::Fast, 16, false));
        sl.hosts[0].address = address.clone();
        sl.statuses.insert(address.clone(), HostStatus::default());
    }

    // Initial call: should trigger trace
    check_and_spawn_traceroutes(&state, &mut last_trace_times, now);

    {
        let sl = state.lock().unwrap();
        let status = sl.statuses.get(&address).unwrap();
        assert!(
            status.tracer_in_progress,
            "Tracer should be in progress after first call"
        );
    }

    // Call again immediately: should NOT trigger trace (already in progress)
    check_and_spawn_traceroutes(&state, &mut last_trace_times, now);

    // Finish trace
    {
        let mut sl = state.lock().unwrap();
        sl.statuses.get_mut(&address).unwrap().tracer_in_progress = false;
        last_trace_times.insert(address.clone(), now);
    }

    // Call again immediately: should NOT trigger trace (cooldown)
    check_and_spawn_traceroutes(&state, &mut last_trace_times, now + Duration::from_secs(10));
    {
        let sl = state.lock().unwrap();
        let status = sl.statuses.get(&address).unwrap();
        assert!(
            !status.tracer_in_progress,
            "Tracer should NOT be in progress during cooldown"
        );
    }

    // Trigger "just became down"
    {
        let mut sl = state.lock().unwrap();
        let status = sl.statuses.get_mut(&address).unwrap();
        status.streak_success = false;
        status.streak = STATE_CONFIRMATION_STREAK;
    }

    // Call after cooldown (but before interval): should trigger because it just became down
    check_and_spawn_traceroutes(
        &state,
        &mut last_trace_times,
        now + Duration::from_secs(TRACEROUTE_MIN_COOLDOWN_SEC + 1),
    );
    {
        let sl = state.lock().unwrap();
        let status = sl.statuses.get(&address).unwrap();
        assert!(
            status.tracer_in_progress,
            "Tracer should be in progress after status change"
        );
    }
}

#[test]
fn test_failure_deduction_skips_unknown_hops() {
    let state = Arc::new(Mutex::new(AppState::default()));
    let target_addr = "8.8.8.8".to_string();
    let hop1 = "*".to_string();

    {
        let mut sl = state.lock().unwrap();
        sl.hosts.push(test_host(PingMode::Fast, 16, false));
        sl.hosts[0].address = target_addr.clone();

        let mut status = HostStatus::default();
        status.streak_success = false;
        status.streak = 3;
        status.traceroute_path = vec![hop1.clone(), target_addr.clone()];
        sl.statuses.insert(target_addr.clone(), status);
    }

    deduce_failure_points(&state, Instant::now());

    let sl = state.lock().unwrap();
    let status = sl.statuses.get(&target_addr).unwrap();
    assert_eq!(
        status.failure_point, None,
        "Unknown hop '*' should not be considered a failure point"
    );
}

#[test]
fn test_failure_deduction_multiple_targets_same_node() {
    let state = Arc::new(Mutex::new(AppState::default()));
    let h1_addr = "1.1.1.1".to_string();
    let h2_addr = "2.2.2.2".to_string();
    let gateway = "10.0.0.1".to_string();
    let common_hop = "9.9.9.9".to_string();

    {
        let mut sl = state.lock().unwrap();
        // Gateway is UP
        let mut g_s = HostStatus::default();
        g_s.streak_success = true;
        g_s.streak = 10;
        g_s.last_updated = Some(Instant::now());
        sl.statuses.insert(gateway.clone(), g_s);

        // Host 1
        sl.hosts.push(test_host(PingMode::Fast, 16, false));
        sl.hosts[0].address = h1_addr.clone();
        let mut s1 = HostStatus::default();
        s1.streak_success = false;
        s1.streak = 3;
        s1.traceroute_path = vec![gateway.clone(), common_hop.clone(), h1_addr.clone()];
        sl.statuses.insert(h1_addr.clone(), s1);

        // Host 2
        sl.hosts.push(test_host(PingMode::Fast, 16, false));
        sl.hosts[1].address = h2_addr.clone();
        let mut s2 = HostStatus::default();
        s2.streak_success = false;
        s2.streak = 3;
        s2.traceroute_path = vec![gateway.clone(), common_hop.clone(), h2_addr.clone()];
        sl.statuses.insert(h2_addr.clone(), s2);

        // Common hop is broken
        let mut hop_s = HostStatus::default();
        hop_s.streak_success = false;
        hop_s.streak = 3;
        hop_s.last_updated = Some(Instant::now());
        sl.statuses.insert(common_hop.clone(), hop_s);
    }

    deduce_failure_points(&state, Instant::now());

    let sl = state.lock().unwrap();
    assert_eq!(
        sl.statuses.get(&h1_addr).unwrap().failure_point,
        Some(common_hop.clone())
    );
    assert_eq!(
        sl.statuses.get(&h2_addr).unwrap().failure_point,
        Some(common_hop.clone())
    );
}
