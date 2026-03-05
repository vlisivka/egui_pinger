use super::*;

#[test]
fn test_add_sample_stats() {
    let mut status = HostStatus::default();
    status.add_sample(10.0, true);
    status.add_sample(20.0, true);
    status.add_sample(f64::NAN, false);

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
    assert_eq!(calculate_percentile(data.iter().copied(), 0.0), 1.0);
    assert_eq!(calculate_percentile(data.iter().copied(), 50.0), 3.0);
    assert_eq!(calculate_percentile(data.iter().copied(), 100.0), 5.0);
    assert_eq!(calculate_percentile(data.iter().copied(), 25.0), 2.0); // (0.25 * 4) = 1.0 -> idx 1 -> 2.0

    let data2 = vec![10.0, 20.0];
    assert_eq!(calculate_percentile(data2.iter().copied(), 50.0), 15.0); // interpolation
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
    status.add_sample(10.0, true);
    status.add_sample(10.0, true);
    status.add_sample(10.0, true);
    assert_eq!(status.streak, 3);
    assert_eq!(status.streak_success, true);

    // Switch to fail streak
    status.add_sample(f64::NAN, false);
    assert_eq!(status.streak, 1);
    assert_eq!(status.streak_success, false);

    status.add_sample(f64::NAN, false);
    assert_eq!(status.streak, 2);
    assert_eq!(status.streak_success, false);

    // Switch back to success
    status.add_sample(10.0, true);
    assert_eq!(status.streak, 1);
    assert_eq!(status.streak_success, true);
}

#[test]
fn test_outliers_detection() {
    let mut status = HostStatus::default();
    // Establish stable baseline
    for _ in 0..10 {
        status.add_sample(10.0, true);
    }
    assert_eq!(status.outliers, 0);
    assert!(status.stddev < 0.1);

    // Add some variation to make stddev > 0.1
    status.add_sample(11.0, true);
    status.add_sample(9.0, true);

    // Threshold is mean + 3*std
    // Initially stddev=0, then we add 11.0.
    // With 10 samples of 10.0 and one 11.0, stddev is small enough that 11.0 might be an outlier.
    // Let's check status.outliers after the spike.
    status.add_sample(100.0, true);
    assert!(status.outliers >= 1);

    let prev_outliers = status.outliers;
    // Another normal sample
    status.add_sample(10.1, true);
    assert_eq!(status.outliers, prev_outliers);
}

#[test]
fn test_advanced_stats() {
    let mut status = HostStatus::default();
    for &rtt in &[10.0, 20.0, 30.0, 40.0, 50.0] {
        status.add_sample(rtt, true);
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
        status.add_sample(i as f64, true);
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
        log_to_file: false,
        log_file_path: String::new(),
        is_stopped: false,
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
        log_to_file: false,
        log_file_path: String::new(),
        is_stopped: false,
    };
    assert_eq!(h.mode, PingMode::Fast);
    assert_eq!(h.packet_size, 16);
}

#[test]
fn test_streak_calculation_complex() {
    let mut status = HostStatus::default();

    status.add_sample(10.0, true);
    assert!(status.streak_success);
    assert_eq!(status.streak, 1);

    status.add_sample(20.0, true);
    assert!(status.streak_success);
    assert_eq!(status.streak, 2);

    status.add_sample(f64::NAN, false);
    assert!(!status.streak_success);
    assert_eq!(status.streak, 1);

    status.add_sample(f64::NAN, false);
    assert!(!status.streak_success);
    assert_eq!(status.streak, 2);

    status.add_sample(10.0, true);
    assert!(status.streak_success);
    assert_eq!(status.streak, 1);
}

#[test]
fn test_large_rtt_history_and_statistics() {
    let mut status = HostStatus::default();
    for i in 1..=500 {
        status.add_sample(i as f64, true);
    }

    assert_eq!(status.history.len(), 300);
    assert_eq!(*status.history.front().unwrap(), 201.0);
    assert_eq!(*status.history.back().unwrap(), 500.0);

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
    status.add_sample(f64::NAN, false);
    status.add_sample(f64::NAN, false);
    status.add_sample(f64::NAN, false);

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
    status.add_sample(42.0, true);

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
    status.add_sample(f64::NAN, false);
    status.add_sample(f64::NAN, false);
    status.add_sample(f64::NAN, false);
    status.add_sample(50.0, true);

    assert_eq!(status.sent, 4);
    assert_eq!(status.lost, 3);
    assert_eq!(status.mean, 50.0);
    assert_eq!(status.median, 50.0);
}

#[test]
fn test_rtp_jitter_calculation() {
    // Verify RFC 3550 jitter calculation
    let mut status = HostStatus::default();
    status.add_sample(100.0, true);
    status.add_sample(110.0, true);

    // First jitter: D = |110 - 100| = 10.0, initial jitter = D = 10.0
    assert_eq!(status.rtp_jitter, 10.0);

    status.add_sample(105.0, true);
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
        status.add_sample((i % 50) as f64 * 10.0, true);
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
    assert_eq!(calculate_percentile(std::iter::empty::<f64>(), 50.0), 0.0);
}

#[test]
fn test_calculate_percentile_single() {
    assert_eq!(calculate_percentile([42.0].iter().copied(), 0.0), 42.0);
    assert_eq!(calculate_percentile([42.0].iter().copied(), 50.0), 42.0);
    assert_eq!(calculate_percentile([42.0].iter().copied(), 100.0), 42.0);
}

#[test]
fn test_availability_always_percentage() {
    let mut status = HostStatus::default();
    // 5 success, 5 fail
    for _ in 0..5 {
        status.add_sample(10.0, true);
    }
    for _ in 0..5 {
        status.add_sample(f64::NAN, false);
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
        log_to_file: false,
        log_file_path: String::new(),
        is_stopped: false,
    };

    let json = serde_json::to_string(&host).unwrap();
    let restored: HostInfo = serde_json::from_str(&json).unwrap();

    assert_eq!(host.name, restored.name);
    assert_eq!(host.address, restored.address);
    assert_eq!(host.mode, restored.mode);
    assert_eq!(host.packet_size, restored.packet_size);
    assert_eq!(host.random_padding, restored.random_padding);
    assert_eq!(host.is_stopped, restored.is_stopped);
}

#[test]
fn test_hostinfo_serde_defaults() {
    // When deserializing old JSON without new fields, defaults should apply
    let json = r#"{"name":"Old","address":"1.1.1.1"}"#;
    let host: HostInfo = serde_json::from_str(json).unwrap();

    assert_eq!(host.mode, PingMode::Fast); // default_ping_mode
    assert_eq!(host.packet_size, 16); // default_packet_size
    assert!(!host.random_padding); // default_false
    assert!(!host.is_stopped); // default_false
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
        log_to_file: false,
        log_file_path: String::new(),
        is_stopped: false,
    });
    state.hosts.push(HostInfo {
        name: "Router".to_string(),
        address: "192.168.1.1".to_string(),
        mode: PingMode::VeryFast,
        display: DisplaySettings::default(),
        packet_size: 16,
        random_padding: false,
        log_to_file: false,
        log_file_path: String::new(),
        is_stopped: false,
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

#[test]
fn test_event_log_limit() {
    let mut status = HostStatus::default();
    for i in 0..100_005 {
        status.events.push_back(LogEntry::Ping {
            timestamp: i,
            seq: i as u32,
            rtt: Some(10.0),
            bytes: 16,
        });
        if status.events.len() > 100_000 {
            status.events.pop_front();
        }
    }
    assert_eq!(status.events.len(), 100_000);
    if let Some(LogEntry::Ping { timestamp, .. }) = status.events.front() {
        assert_eq!(*timestamp, 5);
    }
}

#[test]
fn test_log_entry_statistics_formatting() {
    let entry = LogEntry::Statistics {
        timestamp: 1672531200, // Fixed time to avoid test flakiness, though format uses local timezone.
        mean: 10.0,
        median: 10.0,
        p95: 15.0,
        jitter: 1.0,
        mos: 4.4,
        loss: 0.0,
        sent: 100,
        lost: 0,
        rtp_mean_jitter: 1.2,
        rtp_median_jitter: 1.1,
        availability: 100.0,
        outliers: 2,
        streak: 100,
        stddev: 0.5,
        min_rtt: 5.0,
        max_rtt: 20.0,
    };

    let mut display = DisplaySettings {
        show_name: true,
        show_address: true,
        show_latency: true,
        show_mean: true,
        show_median: true,
        show_rtp_jitter: true,
        show_rtp_mean_jitter: true,
        show_rtp_median_jitter: true,
        show_mos: true,
        show_availability: true,
        show_outliers: true,
        show_streak: true,
        show_stddev: true,
        show_p95: true,
        show_min_max: true,
        show_loss: true,
    };

    let formatted_all = entry.format("127.0.0.1", Some(&display));
    assert!(formatted_all.contains("M="));
    assert!(formatted_all.contains("Med="));
    assert!(formatted_all.contains("95%="));
    assert!(formatted_all.contains("J="));
    assert!(formatted_all.contains("Jm="));
    assert!(formatted_all.contains("Jmed="));
    assert!(formatted_all.contains("MOS="));
    assert!(formatted_all.contains("Av="));
    assert!(formatted_all.contains("Out="));
    assert!(formatted_all.contains("Str="));
    assert!(formatted_all.contains("SD="));
    assert!(formatted_all.contains("m/M="));
    assert!(formatted_all.contains("L:")); // Because of translated loss

    // Test with all disabled
    display.show_mean = false;
    display.show_median = false;
    display.show_rtp_jitter = false;
    display.show_rtp_mean_jitter = false;
    display.show_rtp_median_jitter = false;
    display.show_mos = false;
    display.show_availability = false;
    display.show_outliers = false;
    display.show_streak = false;
    display.show_stddev = false;
    display.show_p95 = false;
    display.show_min_max = false;
    display.show_loss = false;

    let formatted_none = entry.format("127.0.0.1", Some(&display));
    assert!(!formatted_none.contains("M="));
    assert!(!formatted_none.contains("Av="));
    assert!(formatted_none.contains("L:")); // Fallback
}

#[test]
fn test_mos_uses_sliding_window() {
    let mut status = HostStatus::default();

    // 100 losses that will be pushed out of the window
    for _ in 0..100 {
        status.add_sample(f64::NAN, false);
    }

    // 300 successes (fills the sliding window)
    for _ in 0..300 {
        status.add_sample(10.0, true);
    }

    // The window only sees successes. Availability should be 100%.
    assert_eq!(status.availability, 100.0);
    // Loss % for MOS should be 0%, so MOS > 4.4
    assert!(status.mos > 4.4, "MOS was {}", status.mos);
}

#[test]
fn test_first_packet_stats() {
    let mut status = HostStatus::default();
    status.add_sample(50.0, true);

    // Stats for a single successful packet
    assert_eq!(status.min_rtt, 50.0);
    assert_eq!(status.max_rtt, 50.0);
    assert_eq!(status.stddev, 0.0);
    assert_eq!(status.outliers, 0);
}

#[test]
fn test_jitter_startup() {
    let mut status = HostStatus::default();

    // First packet lost (sent = 1)
    status.add_sample(f64::NAN, false);

    // Next two successful packets (sent = 2, 3)
    status.add_sample(20.0, true);
    status.add_sample(30.0, true);

    // Difference is exactly 10.0. It should initialize jitter to exactly 10.0,
    // not (10.0 - 0.0) / 16.0 = 0.625
    assert_eq!(status.rtp_jitter, 10.0);
}

#[test]
fn test_all_stats_use_sliding_window() {
    let mut status = HostStatus::default();

    // Fill the window with bad data (300 pings)
    // - Poor RTT, High Jitter (varying RTT to ensure stddev/outliers)
    for i in 0..300 {
        if i % 2 == 0 {
            status.add_sample(300.0, true);
        } else {
            // Drop some to lower availability
            status.add_sample(f64::NAN, false);
        }
    }

    assert!(status.availability < 100.0);
    assert!(status.mos < 4.0);
    assert!(status.mean >= 300.0);

    // Now push exactly window-size (300) of perfect data
    // to strictly flush the old bad data out of `history` and `rtp_jitter_history`.
    for _ in 0..300 {
        status.add_sample(10.0, true);
    }

    // Check global counters (they SHOULD remember)
    assert_eq!(status.sent, 600);
    assert_eq!(status.lost, 150);

    // Check sliding window metrics (they should be PERFECT)
    assert_eq!(status.availability, 100.0);
    assert!(status.mos > 4.4, "MOS was {}", status.mos);
    assert_eq!(status.mean, 10.0);
    assert_eq!(status.median, 10.0);
    assert_eq!(status.p95, 10.0);
    assert_eq!(status.min_rtt, 10.0);
    assert_eq!(status.max_rtt, 10.0);
    // StdDev shouldn't have old outliers
    assert_eq!(status.stddev, 0.0);
    assert_eq!(status.outliers, 0);

    // RTP jitter requires 1 packet to transition, but after 300 flat identical intervals,
    // it will have decayed completely.
    assert!(status.rtp_jitter < 0.1);
    // The history contains the decay curve from 290.0 down to 0, so the mean over those 300 elements
    // won't be practically 0, but it will be quite small (around ~0.96).
    assert!(status.rtp_jitter_mean < 1.0);
    assert!(status.rtp_jitter_median < 1.0);
}

#[test]
fn test_reset_statistics() {
    let mut status = HostStatus::default();
    status.add_sample(100.0, true);
    status.add_sample(110.0, true);
    status.add_sample(f64::NAN, false);
    status.events.push_back(crate::model::LogEntry::Marker {
        timestamp: 123,
        message: "Test".to_string(),
    });

    assert_eq!(status.sent, 3);
    assert_eq!(status.events.len(), 1);
    assert!(!status.history.is_empty());

    status.reset_statistics();

    assert_eq!(status.sent, 0);
    assert_eq!(status.lost, 0);
    assert!(status.history.is_empty());
    assert!(status.events.is_empty());
    assert!(status.latency.is_nan());
    assert_eq!(status.mean, 0.0);
    assert_eq!(status.rtp_jitter, 0.0);
}
