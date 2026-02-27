use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use egui_pinger::app::EguiPinger;
use egui_pinger::model::*;
use std::sync::{Arc, Mutex};
use tr::tr;

// --- Helper ---

fn make_state_with_host(name: &str, address: &str, mode: PingMode) -> (Arc<Mutex<AppState>>, ()) {
    let state = Arc::new(Mutex::new(AppState::default()));
    {
        let mut s = state.lock().unwrap();
        s.hosts.push(HostInfo {
            name: name.to_string(),
            address: address.to_string(),
            mode,
            display: DisplaySettings::default(),
            packet_size: 16,
            random_padding: false,
        });
        s.statuses
            .insert(address.to_string(), HostStatus::default());
    }
    (state, ())
}

fn make_state_with_active_host(name: &str, address: &str, rtt: f64) -> Arc<Mutex<AppState>> {
    let state = Arc::new(Mutex::new(AppState::default()));
    {
        let mut s = state.lock().unwrap();
        s.hosts.push(HostInfo {
            name: name.to_string(),
            address: address.to_string(),
            mode: PingMode::Fast,
            display: DisplaySettings {
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
            },
            packet_size: 64,
            random_padding: true,
        });
        let mut status = HostStatus::default();
        status.alive = true;
        status.latency = rtt;
        // Add a few samples for realistic stats
        status.add_sample(rtt);
        status.add_sample(rtt + 5.0);
        status.add_sample(rtt - 3.0);
        s.statuses.insert(address.to_string(), status);
    }
    state
}

// === Basic CRUD tests ===

#[test]
fn test_add_host_flow() {
    let state = Arc::new(Mutex::new(AppState::default()));
    let mut app = EguiPinger::from_state(state.clone());
    app.input_name = "Google".to_string();
    app.input_address = "8.8.8.8".to_string();

    let mut harness = Harness::new(|ctx| app.ui_layout(ctx));

    harness.get_by_label("Add").click();
    harness.run();

    let state_lock = state.lock().unwrap();
    assert_eq!(state_lock.hosts.len(), 1);
    assert_eq!(state_lock.hosts[0].name, "Google");
    assert_eq!(state_lock.hosts[0].address, "8.8.8.8");
}

#[test]
fn test_remove_host_flow() {
    let (state, _) = make_state_with_host("Test", "1.2.3.4", PingMode::Fast);

    let mut app = EguiPinger::from_state(state.clone());
    let mut harness = Harness::new(|ctx| app.ui_layout(ctx));
    harness.set_size(egui::vec2(1200.0, 800.0));
    harness.run();

    assert_eq!(state.lock().unwrap().hosts.len(), 1);

    harness.get_by_label("x").click();
    harness.run();

    harness.get_by_label(&tr!("Delete")).click();
    harness.run();

    assert!(state.lock().unwrap().hosts.is_empty());
}

#[test]
fn test_validation_empty_address() {
    let state = Arc::new(Mutex::new(AppState::default()));
    let mut app = EguiPinger::from_state(state.clone());
    app.input_name = "Invalid".to_string();
    app.input_address = String::new();

    let mut harness = Harness::new(|ctx| app.ui_layout(ctx));
    harness.get_by_label("Add").click();
    harness.run();

    assert!(state.lock().unwrap().hosts.is_empty());
}

#[test]
fn test_status_display_updates() {
    let state = Arc::new(Mutex::new(AppState::default()));
    {
        let mut s = state.lock().unwrap();
        s.hosts.push(HostInfo {
            name: "Google".to_string(),
            address: "8.8.8.8".to_string(),
            mode: PingMode::Fast,
            display: DisplaySettings::default(),
            packet_size: 16,
            random_padding: false,
        });
        let mut status = HostStatus::default();
        status.alive = true;
        status.latency = 123.0;
        status.mean = 123.0;
        s.statuses.insert("8.8.8.8".to_string(), status);
    }

    let mut app = EguiPinger::from_state(state.clone());
    let mut harness = Harness::new(|ctx| app.ui_layout(ctx));
    harness.run();

    harness.get_by_label_contains("123ms");
}

// === Duplicate host prevention ===

#[test]
fn test_duplicate_host_not_added() {
    let state = Arc::new(Mutex::new(AppState::default()));
    let mut app = EguiPinger::from_state(state.clone());

    // Add first host
    app.input_name = "First".to_string();
    app.input_address = "8.8.8.8".to_string();
    {
        let mut harness = Harness::new(|ctx| app.ui_layout(ctx));
        harness.get_by_label("Add").click();
        harness.run();
    }

    assert_eq!(state.lock().unwrap().hosts.len(), 1);

    // Try to add the same address again
    app.input_name = "Second".to_string();
    app.input_address = "8.8.8.8".to_string();
    {
        let mut harness = Harness::new(|ctx| app.ui_layout(ctx));
        harness.get_by_label("Add").click();
        harness.run();
    }

    // Should still be 1
    assert_eq!(state.lock().unwrap().hosts.len(), 1);
}

// === Whitespace trimming ===

#[test]
fn test_whitespace_trimmed() {
    let state = Arc::new(Mutex::new(AppState::default()));
    let mut app = EguiPinger::from_state(state.clone());
    app.input_name = "  Google  ".to_string();
    app.input_address = "  8.8.8.8  ".to_string();

    let mut harness = Harness::new(|ctx| app.ui_layout(ctx));
    harness.get_by_label("Add").click();
    harness.run();

    let state_lock = state.lock().unwrap();
    assert_eq!(state_lock.hosts[0].name, "Google");
    assert_eq!(state_lock.hosts[0].address, "8.8.8.8");
}

// === Local host detection ===

#[test]
fn test_local_host_gets_fast_mode() {
    let state = Arc::new(Mutex::new(AppState::default()));
    let mut app = EguiPinger::from_state(state.clone());
    app.input_name = "Router".to_string();
    app.input_address = "192.168.1.1".to_string();

    let mut harness = Harness::new(|ctx| app.ui_layout(ctx));
    harness.get_by_label("Add").click();
    harness.run();

    let state_lock = state.lock().unwrap();
    assert_eq!(state_lock.hosts[0].mode, PingMode::Fast);
}

#[test]
fn test_remote_host_gets_slow_mode() {
    let state = Arc::new(Mutex::new(AppState::default()));
    let mut app = EguiPinger::from_state(state.clone());
    app.input_name = "Google".to_string();
    app.input_address = "8.8.8.8".to_string();

    let mut harness = Harness::new(|ctx| app.ui_layout(ctx));
    harness.get_by_label("Add").click();
    harness.run();

    let state_lock = state.lock().unwrap();
    assert_eq!(state_lock.hosts[0].mode, PingMode::Slow);
}

// === Down host display ===

#[test]
fn test_down_host_shows_down_label() {
    let (state, _) = make_state_with_host("Dead", "10.0.0.99", PingMode::Fast);
    // Status is default → alive=false

    let mut app = EguiPinger::from_state(state.clone());
    let mut harness = Harness::new(|ctx| app.ui_layout(ctx));
    harness.set_size(egui::vec2(1200.0, 800.0));
    harness.run();

    harness.get_by_label_contains("DOWN");
}

// === All stats visible ===

#[test]
fn test_all_stats_rendered() {
    let state = make_state_with_active_host("Server", "10.0.0.1", 50.0);

    let mut app = EguiPinger::from_state(state.clone());
    let mut harness = Harness::new(|ctx| app.ui_layout(ctx));
    harness.set_size(egui::vec2(2000.0, 800.0));
    harness.run();

    // Verify stat labels are present. We use query_all and count because multiple instances might exist
    // (e.g. if the plot also contains text or if we have multiple hosts, though here we have one).
    assert!(
        harness
            .query_all(egui_kittest::kittest::By::new().label_contains("M:"))
            .count()
            > 0
    );
    assert!(
        harness
            .query_all(egui_kittest::kittest::By::new().label_contains("Med:"))
            .count()
            > 0
    );
    assert!(
        harness
            .query_all(egui_kittest::kittest::By::new().label_contains("J:"))
            .count()
            > 0
    );
    assert!(
        harness
            .query_all(egui_kittest::kittest::By::new().label_contains("MOS:"))
            .count()
            > 0
    );
    assert!(
        harness
            .query_all(egui_kittest::kittest::By::new().label_contains("L:"))
            .count()
            > 0
    );
}

// === Cancel deletion ===

#[test]
fn test_cancel_deletion() {
    let (state, _) = make_state_with_host("Keep", "5.5.5.5", PingMode::Fast);

    let mut app = EguiPinger::from_state(state.clone());
    let mut harness = Harness::new(|ctx| app.ui_layout(ctx));
    harness.set_size(egui::vec2(1200.0, 800.0));
    harness.run();

    // Click delete
    harness.get_by_label("x").click();
    harness.run();

    // Click Cancel instead of Delete
    harness.get_by_label(&tr!("Cancel")).click();
    harness.run();

    // Host should still be there
    assert_eq!(state.lock().unwrap().hosts.len(), 1);
}

// === Multiple hosts ===

#[test]
fn test_multiple_hosts() {
    let state = Arc::new(Mutex::new(AppState::default()));
    let mut app = EguiPinger::from_state(state.clone());

    // Add first
    app.input_name = "Google".to_string();
    app.input_address = "8.8.8.8".to_string();
    {
        let mut harness = Harness::new(|ctx| app.ui_layout(ctx));
        harness.get_by_label("Add").click();
        harness.run();
    }

    // Add second
    app.input_name = "CF".to_string();
    app.input_address = "1.1.1.1".to_string();
    {
        let mut harness = Harness::new(|ctx| app.ui_layout(ctx));
        harness.get_by_label("Add").click();
        harness.run();
    }

    let state_lock = state.lock().unwrap();
    assert_eq!(state_lock.hosts.len(), 2);
    assert_eq!(state_lock.hosts[0].address, "8.8.8.8");
    assert_eq!(state_lock.hosts[1].address, "1.1.1.1");
}

// === Settings window opens ===

#[test]
fn test_settings_window_opens() {
    let (state, _) = make_state_with_host("TestHost", "3.3.3.3", PingMode::Fast);

    let mut app = EguiPinger::from_state(state.clone());
    let mut harness = Harness::new(|ctx| app.ui_layout(ctx));
    harness.set_size(egui::vec2(1200.0, 800.0));
    harness.run();

    // Click settings button
    harness.get_by_label("⚙").click();
    harness.run();

    // Settings window should show host address
    assert!(
        harness
            .query_all(egui_kittest::kittest::By::new().label_contains("3.3.3.3"))
            .count()
            > 0
    );
    // And various checkboxes
    assert!(
        harness
            .query_all(egui_kittest::kittest::By::new().label_contains("Host Name"))
            .count()
            > 0
    );
}

// === Empty name is allowed ===

#[test]
fn test_empty_name_allowed() {
    let state = Arc::new(Mutex::new(AppState::default()));
    let mut app = EguiPinger::from_state(state.clone());
    app.input_name = String::new();
    app.input_address = "9.9.9.9".to_string();

    let mut harness = Harness::new(|ctx| app.ui_layout(ctx));
    harness.get_by_label("Add").click();
    harness.run();

    let state_lock = state.lock().unwrap();
    assert_eq!(state_lock.hosts.len(), 1);
    assert_eq!(state_lock.hosts[0].name, "");
    assert_eq!(state_lock.hosts[0].address, "9.9.9.9");
}

// === VPN protection defaults ===

#[test]
fn test_new_host_has_vpn_defaults() {
    let state = Arc::new(Mutex::new(AppState::default()));
    let mut app = EguiPinger::from_state(state.clone());
    app.input_name = "Test".to_string();
    app.input_address = "8.8.4.4".to_string();

    let mut harness = Harness::new(|ctx| app.ui_layout(ctx));
    harness.get_by_label("Add").click();
    harness.run();

    let state_lock = state.lock().unwrap();
    let host = &state_lock.hosts[0];
    assert_eq!(host.packet_size, 16);
    assert!(!host.random_padding);
}
