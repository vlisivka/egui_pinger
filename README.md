# egui_pinger

A powerful network diagnostic tool with a graphical interface designed for Windows and Linux, built with Rust and egui. It is specifically optimized for detecting and diagnosing network issues that affect **IP telephony (VoIP/SIP)** quality.

## Features

- **Multi-host Monitoring**: Periodically ping multiple servers (e.g., Google DNS, Cloudflare, and your SIP server) simultaneously.
- **Advanced Network Analysis**:
  - **RTP Jitter (RFC 3550)**: Standard industrial jitter calculation used in VoIP.
  - **MOS (Mean Opinion Score)**: Estimated voice quality score (1.0 - 4.5).
  - **Latency Metrics**: Real-time tracking of Mean, Median, P95, StdDev, and Min/Max RTT.
  - **Outlier Detection**: Automatic identification of "lags" (packets with >3*StdDev deviation).
- **Privacy & Security**:
  - **Interval Jitter**: Randomized delays (±10%) to break traffic periodicity and avoid fingerprinting.
  - **Packet Padding**: Configurable and randomized packet sizes to mask the nature of ICMP traffic.
- **Visual Excellence**:
  - **Sparkline Charts**: Real-time history for the last 300 pings with a 150ms "warning" line.
  - **Dynamic Coloring**: Color-blind friendly palette (Okabe-Ito) for status and latency alerts.
  - **Rich Tooltips**: Detailed explanations and data for every metric on hover.
- **Power-User UI**:
  - **Drag & Drop**: Reorder host rows easily.
  - **Column Customization**: Select which metrics to display for each host individually.
  - **Theme Support**: Adaptive dark/light mode following system settings.
- **Data Persistence**: Automatically saves host lists and individual display settings.

## Roadmap & Progress

- [x] **Phase 1: Core**: Async ICMP, Multi-host, Persistence.
- [x] **Phase 2: Advanced Logic**: RFC 3550 Jitter, MOS Score, Outlier detection.
- [x] **Phase 3: Privacy & VPN**: Packet padding, interval jittering, traffic masking.
- [x] **Phase 4: UI/UX**: Drag & Drop, Sparklines, Theme support, Column customization.
- [x] **Phase 5: I18n**: Support for English and Ukrainian (including Windows without C-dependencies).
- [ ] **Phase 6: Native Diagnostics**: Integrated `mtr`/`traceroute` triggers on failure (In Progress).

## Technical Stack

- **Language**: [Rust](https://www.rust-lang.org/) (Edition 2024)
- **UI Framework**: [egui](https://github.com/emilk/egui) / [eframe](https://github.com/emilk/egui/tree/master/crates/eframe)
- **Async Runtime**: [Tokio](https://tokio.rs/)
- **I18n**: `tr` crate with `mo-translator` (Pure Rust backend for Windows).

## Building from Source

### Native Build (Linux/Windows)
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

### Cross-Compilation (Linux to Windows MSVC)
The project includes a `build-releases.sh` script that supports cross-compilation using `cargo-xwin` for MSVC targets.

**Requirements**: `clang`, `lld`, `llvm` (providing `llvm-lib`).

```bash
# On AlmaLinux/Fedora/RHEL:
sudo dnf install clang lld llvm
# Run the release script:
./build-releases.sh
```

## Documentation
Technical specifications, detailed plans, and the project TODO list are maintained in Ukrainian (see `Специфікація.md` and `TODO.md` in the root directory).
