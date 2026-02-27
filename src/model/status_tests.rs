use super::*;

#[test]
fn test_add_sample_stats() {
    let mut status = HostStatus::default();
    status.add_sample(10.0);
    status.add_sample(20.0);
    status.add_sample(f64::NAN);

    assert_eq!(status.sent, 3);
    assert_eq!(status.lost, 1);
    assert_eq!(status.mean, 15.0); // (10+20)/2
    assert_eq!(status.availability, (2.0 / 3.0) * 100.0);
    assert_eq!(status.streak, 1);
    assert_eq!(status.streak_success, false); // Last was NaN
}

#[test]
fn test_calculate_percentile() {
    let data = vec![1.0, 2.0, 3.0, 4.0, 5.0];
    assert_eq!(calculate_percentile(&data, 0.0), 1.0);
    assert_eq!(calculate_percentile(&data, 50.0), 3.0);
    assert_eq!(calculate_percentile(&data, 100.0), 5.0);
    assert_eq!(calculate_percentile(&data, 25.0), 2.0); // (0.25 * 4) = 1.0 -> idx 1 -> 2.0

    let data2 = vec![10.0, 20.0];
    assert_eq!(calculate_percentile(&data2, 50.0), 15.0); // interpolation
}

#[test]
fn test_calculate_mos_values() {
    // Ideal network: Low RTT, no jitter, no loss
    let excellent = calculate_mos(10.0, 0.0, 0.0);
    assert!(excellent > 4.4);

    // Typical good network: 50ms RTT, 5ms jitter, 0% loss
    let good = calculate_mos(50.0, 5.0, 0.0);
    assert!(good > 4.0 && good < 4.4);

    // Degraded network: 150ms RTT, 20ms jitter, 1% loss
    let stressed = calculate_mos(150.0, 20.0, 1.0);
    // Effective latency 200ms -> R ~ 83.7 -> MOS ~ 4.1
    assert!(stressed < 4.2 && stressed > 3.0);

    // Bad network: 300ms RTT, 50ms jitter, 5% loss
    let bad = calculate_mos(300.0, 50.0, 5.0);
    assert!(bad < 3.0);
}

#[test]
fn test_streaks() {
    let mut status = HostStatus::default();

    // Success streak
    status.add_sample(10.0);
    status.add_sample(10.0);
    status.add_sample(10.0);
    assert_eq!(status.streak, 3);
    assert_eq!(status.streak_success, true);

    // Switch to fail streak
    status.add_sample(f64::NAN);
    assert_eq!(status.streak, 1);
    assert_eq!(status.streak_success, false);

    status.add_sample(f64::NAN);
    assert_eq!(status.streak, 2);
    assert_eq!(status.streak_success, false);

    // Switch back to success
    status.add_sample(10.0);
    assert_eq!(status.streak, 1);
    assert_eq!(status.streak_success, true);
}

#[test]
fn test_outliers_detection() {
    let mut status = HostStatus::default();
    // Establish stable baseline
    for _ in 0..10 {
        status.add_sample(10.0);
    }
    assert_eq!(status.outliers, 0);
    assert!(status.stddev < 0.1);

    // Add some variation to make stddev > 0.1
    status.add_sample(11.0);
    status.add_sample(9.0);

    // Threshold is mean + 3*std
    // Initially stddev=0, then we add 11.0.
    // With 10 samples of 10.0 and one 11.0, stddev is small enough that 11.0 might be an outlier.
    // Let's check status.outliers after the spike.
    status.add_sample(100.0);
    assert!(status.outliers >= 1);

    let prev_outliers = status.outliers;
    // Another normal sample
    status.add_sample(10.1);
    assert_eq!(status.outliers, prev_outliers);
}

#[test]
fn test_advanced_stats() {
    let mut status = HostStatus::default();
    for &rtt in &[10.0, 20.0, 30.0, 40.0, 50.0] {
        status.add_sample(rtt);
    }

    assert_eq!(status.min_rtt, 10.0);
    assert_eq!(status.max_rtt, 50.0);
    assert_eq!(status.median, 30.0);
    assert!(status.p95 > 40.0);
    assert_eq!(status.mean, 30.0);
}

#[test]
fn test_history_limit() {
    let mut status = HostStatus::default();
    for i in 0..400 {
        status.add_sample(i as f64);
    }
    assert_eq!(status.history.len(), 300);
    assert_eq!(status.history[0], 100.0);
    assert_eq!(status.history[299], 399.0);
}

#[test]
fn test_hostinfo_is_local() {
    let mut h = HostInfo {
        name: "".to_string(),
        address: "127.0.0.1".to_string(),
        mode: PingMode::Fast,
        display: DisplaySettings::default(),
        packet_size: 16,
        random_padding: false,
    };
    assert!(h.is_local(), "127.0.0.1 should be local");

    h.address = "192.168.1.1".to_string();
    assert!(h.is_local(), "192.168.1.1 should be local");

    h.address = "10.0.0.1".to_string();
    assert!(h.is_local(), "10.0.0.1 should be local");

    h.address = "172.16.0.1".to_string();
    assert!(h.is_local(), "172.16.0.1 should be local");

    h.address = "8.8.8.8".to_string();
    assert!(!h.is_local(), "8.8.8.8 should NOT be local");

    h.address = "google.com".to_string();
    assert!(
        !h.is_local(),
        "Domain name should NOT be local by parsing IP"
    );

    h.address = "::1".to_string();
    assert!(h.is_local(), "IPv6 loopback should be local");

    h.address = "fe80::1".to_string();
    assert!(h.is_local(), "IPv6 link local should be local");

    h.address = "fc00::1".to_string();
    assert!(h.is_local(), "IPv6 unique local should be local");

    h.address = "2001:4860:4860::8888".to_string();
    assert!(!h.is_local(), "Public IPv6 should NOT be local");
}

#[test]
fn test_default_display_settings() {
    let d = DisplaySettings::default();
    assert!(d.show_name);
    assert!(d.show_address);
    assert!(d.show_latency);
    assert!(d.show_mean);
    assert!(d.show_median);
    assert!(d.show_rtp_jitter);
    assert!(d.show_loss);
    assert!(d.show_mos);
    // These should be false by default
    assert!(!d.show_rtp_mean_jitter);
    assert!(!d.show_rtp_median_jitter);
    assert!(!d.show_availability);
    assert!(!d.show_outliers);
    assert!(!d.show_streak);
    assert!(!d.show_stddev);
    assert!(!d.show_p95);
    assert!(!d.show_min_max);
}

#[test]
fn test_hostinfo_defaults() {
    let h = HostInfo {
        name: "A".to_string(),
        address: "B".to_string(),
        mode: default_ping_mode(),
        display: DisplaySettings::default(),
        packet_size: default_packet_size(),
        random_padding: false,
    };
    assert_eq!(h.mode, PingMode::Fast);
    assert_eq!(h.packet_size, 16);
}

#[test]
fn test_streak_calculation_complex() {
    let mut status = HostStatus::default();

    status.add_sample(10.0);
    assert!(status.streak_success);
    assert_eq!(status.streak, 1);

    status.add_sample(20.0);
    assert!(status.streak_success);
    assert_eq!(status.streak, 2);

    status.add_sample(f64::NAN);
    assert!(!status.streak_success);
    assert_eq!(status.streak, 1);

    status.add_sample(f64::NAN);
    assert!(!status.streak_success);
    assert_eq!(status.streak, 2);

    status.add_sample(10.0);
    assert!(status.streak_success);
    assert_eq!(status.streak, 1);
}

#[test]
fn test_large_rtt_history_and_statistics() {
    let mut status = HostStatus::default();
    for i in 1..=500 {
        status.add_sample(i as f64);
    }

    assert_eq!(status.history.len(), 300);
    assert_eq!(*status.history.first().unwrap(), 201.0);
    assert_eq!(*status.history.last().unwrap(), 500.0);

    assert_eq!(status.min_rtt, 201.0);
    assert_eq!(status.max_rtt, 500.0);

    let mean = (201.0 + 500.0) / 2.0; // 350.5
    assert_eq!(status.mean, mean);
    assert_eq!(status.median, 350.5);
}

// --- Edge cases for 100% coverage ---

#[test]
fn test_all_nan_samples() {
    // Covers the early return when valid_data is empty (lines 225-228)
    let mut status = HostStatus::default();
    status.add_sample(f64::NAN);
    status.add_sample(f64::NAN);
    status.add_sample(f64::NAN);

    assert_eq!(status.sent, 3);
    assert_eq!(status.lost, 3);
    assert_eq!(status.mean, 0.0);
    assert_eq!(status.median, 0.0);
    assert_eq!(status.availability, 0.0);
    assert!(!status.alive);
}

#[test]
fn test_single_valid_sample() {
    // Covers the early return when valid_data.len() < 2 (lines 231-236)
    let mut status = HostStatus::default();
    status.add_sample(42.0);

    assert_eq!(status.sent, 1);
    assert_eq!(status.lost, 0);
    assert_eq!(status.mean, 42.0);
    assert_eq!(status.median, 42.0);
    assert!(status.mos > 1.0); // MOS should be calculated
    assert_eq!(status.availability, 100.0);
}

#[test]
fn test_single_valid_after_nans() {
    // Edge case: many NaN then one valid — valid_data has exactly 1 element
    let mut status = HostStatus::default();
    status.add_sample(f64::NAN);
    status.add_sample(f64::NAN);
    status.add_sample(f64::NAN);
    status.add_sample(50.0);

    assert_eq!(status.sent, 4);
    assert_eq!(status.lost, 3);
    assert_eq!(status.mean, 50.0);
    assert_eq!(status.median, 50.0);
}

#[test]
fn test_rtp_jitter_calculation() {
    // Verify RFC 3550 jitter calculation
    let mut status = HostStatus::default();
    status.add_sample(100.0);
    status.add_sample(110.0);

    // First jitter: D = |110 - 100| = 10.0, initial jitter = D = 10.0
    assert_eq!(status.rtp_jitter, 10.0);

    status.add_sample(105.0);
    // D = |105 - 110| = 5.0
    // J = 10.0 + (5.0 - 10.0) / 16.0 = 10.0 - 0.3125 = 9.6875
    assert!((status.rtp_jitter - 9.6875).abs() < 0.001);

    // Jitter history should match
    assert_eq!(status.rtp_jitter_history.len(), 2);
}

#[test]
fn test_rtp_jitter_history_limit() {
    // Jitter history should also be capped at 300
    let mut status = HostStatus::default();
    for i in 0..400 {
        status.add_sample((i % 50) as f64 * 10.0);
    }
    assert!(status.rtp_jitter_history.len() <= 300);
}

#[test]
fn test_mos_monotonically_degrades() {
    // MOS should decrease as conditions worsen
    let ideal = calculate_mos(0.0, 0.0, 0.0);
    let good = calculate_mos(50.0, 5.0, 0.0);
    let fair = calculate_mos(100.0, 15.0, 1.0);
    let poor = calculate_mos(200.0, 30.0, 3.0);
    let bad = calculate_mos(400.0, 60.0, 10.0);

    assert!(ideal > good, "Ideal ({ideal}) > Good ({good})");
    assert!(good > fair, "Good ({good}) > Fair ({fair})");
    assert!(fair > poor, "Fair ({fair}) > Poor ({poor})");
    assert!(poor > bad, "Poor ({poor}) > Bad ({bad})");
}

#[test]
fn test_mos_is_clamped() {
    // MOS should always be >= 1.0 and <= 4.5
    let worst = calculate_mos(10000.0, 10000.0, 100.0);
    assert!(worst >= 1.0, "MOS {worst} should be >= 1.0");

    let best = calculate_mos(0.0, 0.0, 0.0);
    assert!(best <= 4.5, "MOS {best} should be <= 4.5");
}

#[test]
fn test_calculate_percentile_empty() {
    assert_eq!(calculate_percentile(&[], 50.0), 0.0);
}

#[test]
fn test_calculate_percentile_single() {
    assert_eq!(calculate_percentile(&[42.0], 0.0), 42.0);
    assert_eq!(calculate_percentile(&[42.0], 50.0), 42.0);
    assert_eq!(calculate_percentile(&[42.0], 100.0), 42.0);
}

#[test]
fn test_availability_always_percentage() {
    let mut status = HostStatus::default();
    // 5 success, 5 fail
    for _ in 0..5 {
        status.add_sample(10.0);
    }
    for _ in 0..5 {
        status.add_sample(f64::NAN);
    }
    assert!((status.availability - 50.0).abs() < 0.01);
}

#[test]
fn test_hostinfo_serde_roundtrip() {
    let host = HostInfo {
        name: "Test Server".to_string(),
        address: "192.168.1.100".to_string(),
        mode: PingMode::NotSlow,
        display: DisplaySettings::default(),
        packet_size: 128,
        random_padding: true,
    };

    let json = serde_json::to_string(&host).unwrap();
    let restored: HostInfo = serde_json::from_str(&json).unwrap();

    assert_eq!(host.name, restored.name);
    assert_eq!(host.address, restored.address);
    assert_eq!(host.mode, restored.mode);
    assert_eq!(host.packet_size, restored.packet_size);
    assert_eq!(host.random_padding, restored.random_padding);
}

#[test]
fn test_hostinfo_serde_defaults() {
    // When deserializing old JSON without new fields, defaults should apply
    let json = r#"{"name":"Old","address":"1.1.1.1"}"#;
    let host: HostInfo = serde_json::from_str(json).unwrap();

    assert_eq!(host.mode, PingMode::Fast); // default_ping_mode
    assert_eq!(host.packet_size, 16); // default_packet_size
    assert!(!host.random_padding); // default_false
}

#[test]
fn test_appstate_serde_roundtrip() {
    use crate::model::AppState;

    let mut state = AppState::default();
    state.hosts.push(HostInfo {
        name: "Google DNS".to_string(),
        address: "8.8.8.8".to_string(),
        mode: PingMode::Slow,
        display: DisplaySettings::default(),
        packet_size: 64,
        random_padding: true,
    });
    state.hosts.push(HostInfo {
        name: "Router".to_string(),
        address: "192.168.1.1".to_string(),
        mode: PingMode::VeryFast,
        display: DisplaySettings::default(),
        packet_size: 16,
        random_padding: false,
    });

    let json = serde_json::to_string_pretty(&state).unwrap();
    let restored: AppState = serde_json::from_str(&json).unwrap();

    assert_eq!(restored.hosts.len(), 2);
    assert_eq!(restored.hosts[0].name, "Google DNS");
    assert_eq!(restored.hosts[0].packet_size, 64);
    assert!(restored.hosts[0].random_padding);
    assert_eq!(restored.hosts[1].name, "Router");
    assert_eq!(restored.hosts[1].mode, PingMode::VeryFast);
}

#[test]
fn test_ping_mode_all_variants() {
    // Ensure all PingMode variants can be serialized and deserialized
    let modes = [
        PingMode::VeryFast,
        PingMode::Fast,
        PingMode::NotFast,
        PingMode::Normal,
        PingMode::NotSlow,
        PingMode::Slow,
        PingMode::VerySlow,
    ];
    for mode in modes {
        let json = serde_json::to_string(&mode).unwrap();
        let restored: PingMode = serde_json::from_str(&json).unwrap();
        assert_eq!(mode, restored, "PingMode roundtrip failed for {:?}", mode);
    }
}
