# egui_pinger

A powerful network diagnostic tool with a graphical interface designed for Windows and Linux, built with Rust and egui. It is specifically optimized for detecting and diagnosing network issues that affect **IP telephony (VoIP/SIP)** quality.

## Features

- **Multi-host Monitoring**: Periodically pin multiple servers (e.g., Google DNS, Cloudflare, and your SIP server) simultaneously.
- **Advanced Network Analysis**:
  - ICMP and UDP-based latency monitoring.
  - Real-time Jitter calculation (short-term and long-term trends).
  - Packet loss percentage tracking.
- **Visual Feedback**: Real-time history graphs for each host to quickly identify spikes or drops.
- **Smart Monitoring Modes**:
  - **Fast mode**: High-frequency polling to catch intermittent issues.
  - **Slow mode**: Background monitoring with reduced overhead and traffic obfuscation.
- **Pattern Masking**: Randomized intervals (jittered delays) between pings to avoid traffic fingerprinting.
- **Incident Diagnostics**: Automatic triggers for `mtr`, `nslookup`, and `traceroute` when anomalies are detected.
- **Data Persistence**: Automatically saves host lists and session data.

## Roadmap

### Phase 1: Core Functionality (Current)
- [x] Basic ICMP pinging with `surge-ping`.
- [x] Multi-host support.
- [x] Real-time latency and jitter (T3, T21, T99) calculation.
- [x] Basic history bar charts.
- [x] Persistence of host lists.

### Phase 2: Enhanced Diagnostics (Next)
- [ ] **UDP Pinging**: Support for specific ports (SIP 5060, etc.) to test firewall/NAT behavior.
- [ ] **DNS Monitoring**: Detect resolution issues and latency in DNS queries.
- [ ] **Expanded Modes**: Implementation of "Fast" and "Slow" modes with configurable randomization.
- [ ] **Auto-Diagnostics**: Triggering external tools (`mtr`, `traceroute`) on packet loss or high jitter.

### Phase 3: Reliability & reporting
- [ ] **Incident Reports**: Generate detailed markdown/JSON reports for network events.
- [ ] **Log Coarsening**: Intelligent log storage that aggregates old data to save space while preserving trends.
- [ ] **Full Windows Support**: Validating and fixing raw socket permissions and tool calls on Windows.

### Phase 4: UX & Polish
- [ ] **Full English Localization**: Switching all UI elements to English.
- [ ] **UI Themes**: Support for dark/light modes and custom styling.
- [ ] **Advanced Graphing**: Zoomable and scrollable history views.

## Technical Stack

- **Language**: [Rust](https://www.rust-lang.org/)
- **UI Framework**: [egui](https://github.com/emilk/egui) / [eframe](https://github.com/emilk/egui/tree/master/crates/eframe)
- **Async Runtime**: [Tokio](https://tokio.rs/)
- **Networking**: `surge-ping` for asynchronous ICMP.

## Building and installation

Ensure you have the Rust toolchain installed.

```bash
git clone https://github.com/vlisivka/egui_pinger && \
cd egui_pinger && \
cargo run --release
```

*Note: On Linux, you may need `CAP_NET_RAW` capabilities to send raw ICMP packets:*
```bash
sudo setcap cap_net_raw+ep target/release/egui_pinger
```

## Documentation
Technical specifications, detailed plans, and the project TODO list are maintained in Ukrainian (see `Специфікація.md` and `TODO.md` in the root directory).
