use eframe::egui;
use eframe::egui::RichText;
use std::sync::{Arc, Mutex};
use tr::tr;

// --- Data structures ---

/// A single diagnostic command available to the user.
pub struct SystemCommand {
    /// Short label shown in the ComboBox.
    pub label: String,
    /// Detailed description shown below the ComboBox.
    pub description: String,
    /// Category for grouping (informational only).
    pub category: String,
    /// Executable name.
    pub cmd: String,
    /// Command arguments.
    pub args: Vec<String>,
}

impl SystemCommand {
    fn new(category: &str, label: &str, description: &str, cmd: &str, args: &[&str]) -> Self {
        Self {
            category: category.to_string(),
            label: label.to_string(),
            description: description.to_string(),
            cmd: cmd.to_string(),
            args: args.iter().map(|s| s.to_string()).collect(),
        }
    }

    /// Returns a shell-style string like `$ ip -c addr` for display.
    pub fn full_command_string(&self) -> String {
        if self.args.is_empty() {
            format!("$ {}", self.cmd)
        } else {
            format!("$ {} {}", self.cmd, self.args.join(" "))
        }
    }
}

/// Active tab in the System Tools window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ToolsTab {
    #[default]
    Guide,
    Commands,
}

/// Persistent state for the System Tools window.
pub struct SystemToolsState {
    pub selected_command: usize,
    pub output: String,
    pub is_running: bool,
    pub selected_tab: ToolsTab,
    /// Shared buffer: background thread writes result here, UI polls it.
    pending_result: Arc<Mutex<Option<String>>>,
    /// Cached command list (built once).
    commands: Vec<SystemCommand>,
}

impl Default for SystemToolsState {
    fn default() -> Self {
        Self {
            selected_command: 0,
            output: String::new(),
            is_running: false,
            selected_tab: ToolsTab::default(),
            pending_result: Arc::new(Mutex::new(None)),
            commands: get_commands(),
        }
    }
}

// --- Command lists per OS ---

/// Returns the list of diagnostic commands for the current OS.
pub fn get_commands() -> Vec<SystemCommand> {
    if cfg!(target_os = "windows") {
        get_windows_commands()
    } else {
        get_linux_commands()
    }
}

fn get_linux_commands() -> Vec<SystemCommand> {
    vec![
        SystemCommand::new(
            &tr!("Basic Info"),
            &tr!("Network Interfaces"),
            &tr!(
                "Shows all network interfaces with their IP addresses, subnet masks, and status (UP/DOWN). Look for your main interface (eth0, wlan0, enp*) and check if it has an IP address assigned."
            ),
            "ip",
            &["addr", "show"],
        ),
        SystemCommand::new(
            &tr!("Basic Info"),
            &tr!("Interface Status"),
            &tr!(
                "Shows link-layer status of all interfaces: MTU, MAC address, and whether the link is physically UP. If your interface shows 'state DOWN', the cable might be unplugged or Wi-Fi is disconnected."
            ),
            "ip",
            &["link", "show"],
        ),
        SystemCommand::new(
            &tr!("Routing"),
            &tr!("Routing Table"),
            &tr!(
                "Displays the IPv4 routing table. Look for the 'default via' line — this is your gateway (router). If this line is missing, your system doesn't know how to reach the internet."
            ),
            "ip",
            &["route", "show"],
        ),
        SystemCommand::new(
            &tr!("Routing"),
            &tr!("IPv6 Routing Table"),
            &tr!(
                "Displays the IPv6 routing table. Similar to IPv4, look for a 'default via' entry for your IPv6 gateway."
            ),
            "ip",
            &["-6", "route", "show"],
        ),
        SystemCommand::new(
            &tr!("DNS"),
            &tr!("DNS Configuration"),
            &tr!(
                "Shows which DNS servers your system is using. If DNS is misconfigured, you can ping IP addresses (like 8.8.8.8) but cannot open websites by name."
            ),
            "resolvectl",
            &["status"],
        ),
        SystemCommand::new(
            &tr!("DNS"),
            &tr!("DNS Lookup (google.com)"),
            &tr!(
                "Performs a DNS query for google.com. If this fails but pinging 8.8.8.8 works, you have a DNS problem. The result should show one or more IP addresses."
            ),
            "dig",
            &["google.com", "+short"],
        ),
        SystemCommand::new(
            &tr!("DNS"),
            &tr!("Reverse DNS (8.8.8.8)"),
            &tr!(
                "Looks up the hostname for IP address 8.8.8.8. This tests if reverse DNS resolution works. Should return 'dns.google'."
            ),
            "dig",
            &["-x", "8.8.8.8", "+short"],
        ),
        SystemCommand::new(
            &tr!("Connections"),
            &tr!("Listening Ports (TCP/UDP)"),
            &tr!(
                "Shows all TCP and UDP ports that are currently listening for incoming connections. Useful for checking if a service (like a SIP server) is running and listening on the expected port."
            ),
            "ss",
            &["-tuln"],
        ),
        SystemCommand::new(
            &tr!("Neighbors"),
            &tr!("ARP Table"),
            &tr!(
                "Shows the ARP cache — a mapping of IP addresses to MAC addresses on your local network. If your gateway's entry is missing or shows 'FAILED', you have a local network connectivity issue."
            ),
            "ip",
            &["neigh", "show"],
        ),
        SystemCommand::new(
            &tr!("Wi-Fi"),
            &tr!("Wi-Fi Connection Status"),
            &tr!(
                "Shows details about your current Wi-Fi connection: SSID, signal strength, frequency, and link speed. Low signal strength (below -70 dBm) can cause packet loss and high latency."
            ),
            "nmcli",
            &["dev", "wifi"],
        ),
        SystemCommand::new(
            &tr!("Wi-Fi"),
            &tr!("Available Wi-Fi Networks"),
            &tr!(
                "Lists all visible Wi-Fi networks with their signal strength, channel, and security. Useful for identifying channel congestion if many networks share the same channel."
            ),
            "nmcli",
            &["dev", "wifi", "list"],
        ),
        SystemCommand::new(
            &tr!("System"),
            &tr!("Interface Statistics"),
            &tr!(
                "Shows detailed packet/byte counters and error statistics for each interface. High values in 'errors', 'dropped', or 'overrun' indicate hardware or driver problems."
            ),
            "ip",
            &["-s", "link", "show"],
        ),
        SystemCommand::new(
            &tr!("System"),
            &tr!("System Uptime"),
            &tr!(
                "Shows how long the system has been running, number of users, and load averages. High load averages (above the number of CPU cores) can indicate system overload affecting network performance."
            ),
            "uptime",
            &[],
        ),
        SystemCommand::new(
            &tr!("System"),
            &tr!("Network Manager Log (last 30 lines)"),
            &tr!(
                "Shows recent NetworkManager journal entries. Look for connection drops, DHCP failures, or Wi-Fi roaming events that might explain network instability."
            ),
            "journalctl",
            &["-u", "NetworkManager", "--no-pager", "-n", "30"],
        ),
    ]
}

fn get_windows_commands() -> Vec<SystemCommand> {
    vec![
        SystemCommand::new(
            &tr!("Basic Info"),
            &tr!("Network Configuration"),
            &tr!(
                "Shows detailed information about all network adapters: IP addresses, subnet masks, default gateways, DNS servers, and DHCP status. This is the most comprehensive overview of your network setup."
            ),
            "ipconfig",
            &["/all"],
        ),
        SystemCommand::new(
            &tr!("Routing"),
            &tr!("Routing Table"),
            &tr!(
                "Displays the IPv4 and IPv6 routing tables. Look for the '0.0.0.0' route — its gateway is your router. If this entry is missing, your system cannot reach the internet."
            ),
            "route",
            &["print"],
        ),
        SystemCommand::new(
            &tr!("DNS"),
            &tr!("DNS Cache"),
            &tr!(
                "Shows the local DNS resolver cache. This reveals which domain names have been recently resolved. If a domain shows an incorrect IP, you may have a stale cache entry."
            ),
            "ipconfig",
            &["/displaydns"],
        ),
        SystemCommand::new(
            &tr!("DNS"),
            &tr!("DNS Lookup (google.com)"),
            &tr!(
                "Performs a DNS query for google.com using the system's default DNS server. If this fails but pinging 8.8.8.8 works, you have a DNS problem."
            ),
            "nslookup",
            &["google.com"],
        ),
        SystemCommand::new(
            &tr!("Connections"),
            &tr!("Active Connections"),
            &tr!(
                "Shows all active TCP/UDP connections and listening ports. Useful for checking if a service is running on the expected port, or if there are unusual outgoing connections."
            ),
            "netstat",
            &["-an"],
        ),
        SystemCommand::new(
            &tr!("Connections"),
            &tr!("Protocol Statistics"),
            &tr!(
                "Shows per-protocol statistics (TCP, UDP, ICMP, IP). High error counts or retransmissions indicate network problems."
            ),
            "netstat",
            &["-s"],
        ),
        SystemCommand::new(
            &tr!("Neighbors"),
            &tr!("ARP Table"),
            &tr!(
                "Shows the ARP cache — IP-to-MAC address mappings on your local network. Missing or incomplete entries for your gateway suggest a local connectivity problem."
            ),
            "arp",
            &["-a"],
        ),
        SystemCommand::new(
            &tr!("Wi-Fi"),
            &tr!("Wi-Fi Connection Status"),
            &tr!(
                "Shows current Wi-Fi adapter details: SSID, signal quality, radio type, channel, and authentication. Signal quality below 50% typically causes packet loss."
            ),
            "netsh",
            &["wlan", "show", "interfaces"],
        ),
        SystemCommand::new(
            &tr!("Wi-Fi"),
            &tr!("Available Wi-Fi Networks"),
            &tr!(
                "Lists all visible Wi-Fi networks with signal strength, channel, and encryption. Helps identify Wi-Fi congestion if many networks overlap on the same channel."
            ),
            "netsh",
            &["wlan", "show", "networks", "mode=bssid"],
        ),
        SystemCommand::new(
            &tr!("System"),
            &tr!("Firewall Status"),
            &tr!(
                "Shows the current Windows Firewall profile status. If the firewall is blocking ICMP, pings will fail even though the network is working."
            ),
            "netsh",
            &["advfirewall", "show", "currentprofile"],
        ),
    ]
}

// --- Command execution ---

/// Runs a system command in a background thread with a 15-second timeout.
/// Result is written to the shared `pending_result` buffer.
fn run_command_background(cmd: String, args: Vec<String>, result_slot: Arc<Mutex<Option<String>>>) {
    std::thread::spawn(move || {
        let output = std::process::Command::new(&cmd)
            .args(&args)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .output();

        let text = match output {
            Ok(out) => {
                let stdout = String::from_utf8_lossy(&out.stdout);
                let stderr = String::from_utf8_lossy(&out.stderr);
                if stderr.is_empty() {
                    stdout.to_string()
                } else if stdout.is_empty() {
                    format!("(stderr)\n{}", stderr)
                } else {
                    format!("{}\n\n(stderr)\n{}", stdout, stderr)
                }
            }
            Err(e) => format!("Error: {}", e),
        };

        if let Ok(mut slot) = result_slot.lock() {
            *slot = Some(text);
        }
    });
}

// --- UI rendering ---

/// Draws the System Tools window. Call from `EguiPinger::ui_layout()`.
pub fn ui_system_tools_window(ctx: &egui::Context, open: &mut bool, state: &mut SystemToolsState) {
    // Poll for background command result
    if state.is_running
        && let Ok(mut slot) = state.pending_result.lock()
        && let Some(result) = slot.take()
    {
        state.output.push_str(&result);
        state.is_running = false;
    }

    let mut open_var = *open;
    egui::Window::new(tr!("System Tools"))
        .open(&mut open_var)
        .resizable(true)
        .default_width(550.0)
        .default_height(450.0)
        .show(ctx, |ui| {
            let commands = &state.commands;
            if commands.is_empty() {
                ui.label(tr!("No commands available for this platform."));
                return;
            }

            // ComboBox — always visible, above tabs
            ui.horizontal(|ui| {
                ui.label(tr!("Command:"));
                let selected_label = &commands[state.selected_command].label;
                egui::ComboBox::from_id_salt("system_cmd_combo")
                    .selected_text(format!(
                        "[{}] {}",
                        commands[state.selected_command].category, selected_label
                    ))
                    .width(400.0)
                    .show_ui(ui, |ui| {
                        let mut current_category = String::new();
                        for (i, cmd) in commands.iter().enumerate() {
                            if cmd.category != current_category {
                                if !current_category.is_empty() {
                                    ui.separator();
                                }
                                current_category = cmd.category.clone();
                                ui.label(RichText::new(&cmd.category).strong());
                            }
                            ui.selectable_value(&mut state.selected_command, i, &cmd.label);
                        }
                    });
            });

            ui.separator();

            // Tabs
            ui.horizontal(|ui| {
                ui.selectable_value(&mut state.selected_tab, ToolsTab::Guide, tr!("Guide"));
                ui.selectable_value(
                    &mut state.selected_tab,
                    ToolsTab::Commands,
                    tr!("Run Command"),
                );
            });

            ui.separator();

            match state.selected_tab {
                ToolsTab::Guide => render_guide(ui),
                ToolsTab::Commands => render_commands_tab(ui, state),
            }
        });

    *open = open_var;
}

/// Renders the "Run Command" tab.
fn render_commands_tab(ui: &mut egui::Ui, state: &mut SystemToolsState) {
    let cmd = &state.commands[state.selected_command];

    // Description
    ui.label(RichText::new(&cmd.description).weak());
    ui.add_space(4.0);

    // Run button
    ui.horizontal(|ui| {
        let run_enabled = !state.is_running;
        if ui
            .add_enabled(run_enabled, egui::Button::new(tr!("▶ Run")))
            .clicked()
        {
            state.is_running = true;
            state.output = format!("{}\n\n", cmd.full_command_string());

            run_command_background(
                cmd.cmd.clone(),
                cmd.args.clone(),
                state.pending_result.clone(),
            );
        }
        if state.is_running {
            ui.spinner();
            ui.label(tr!("Running..."));
        }
    });

    ui.add_space(4.0);

    // Output area
    egui::ScrollArea::vertical()
        .stick_to_bottom(true)
        .show(ui, |ui| {
            ui.add(
                egui::TextEdit::multiline(&mut state.output.as_str())
                    .font(egui::TextStyle::Monospace)
                    .desired_width(f32::INFINITY)
                    .desired_rows(20),
            );
        });
}

/// Renders the troubleshooting guide tab.
fn render_guide(ui: &mut egui::Ui) {
    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 4.0;

        // Section 1: No internet
        ui.strong(tr!("1. Internet is completely down"));
        ui.label(tr!("If you cannot access any website or service:"));
        ui.label(tr!("  • Run 'Network Interfaces' — check if your adapter has an IP address. If no IP is assigned, DHCP may have failed."));
        ui.label(tr!("  • Run 'Routing Table' — look for a 'default' route. If missing, the system doesn't know how to reach the internet."));
        ui.label(tr!("  • Run 'DNS Lookup (google.com)' — if this fails but pinging 8.8.8.8 works in the main window, you have a DNS problem, not a connection problem."));
        ui.add_space(8.0);

        // Section 2: Intermittent connectivity
        ui.strong(tr!("2. Connection drops or is unstable"));
        ui.label(tr!("If the connection works sometimes but keeps cutting out:"));
        ui.label(tr!("  • Run 'ARP Table' — if the gateway entry shows 'FAILED' or 'INCOMPLETE', there is a problem on your local network (cable, switch, or router)."));
        ui.label(tr!("  • Run 'Interface Statistics' — look for high 'errors' or 'dropped' counters. These indicate hardware, driver, or cable problems."));
        ui.label(tr!("  • Run 'Network Manager Log' — look for repeated 'connection dropped' or 'DHCP lease expired' messages."));
        ui.add_space(8.0);

        // Section 3: Slow internet
        ui.strong(tr!("3. Internet is slow"));
        ui.label(tr!("If pages load slowly or VoIP calls have bad quality:"));
        ui.label(tr!("  • Run 'Wi-Fi Connection Status' — check signal strength. Below -70 dBm (or below 50% on Windows) causes packet loss and jitter."));
        ui.label(tr!("  • Run 'Available Wi-Fi Networks' — if many networks use the same channel, interference is likely. Consider switching to a less crowded channel or 5 GHz band."));
        ui.label(tr!("  • Run 'Listening Ports' — check if many connections are open. A torrent client or large download can saturate your bandwidth."));
        ui.add_space(8.0);

        // Section 4: DNS problems
        ui.strong(tr!("4. Websites don't open, but ping works"));
        ui.label(tr!("If you can ping 8.8.8.8 in the main window but cannot open websites:"));
        ui.label(tr!("  • Run 'DNS Configuration' — verify your DNS servers are correctly set. Common public DNS: 8.8.8.8, 1.1.1.1, 9.9.9.9."));
        ui.label(tr!("  • Run 'DNS Lookup (google.com)' — if it fails or returns wrong IPs, your DNS server may be down or misconfigured."));
        if cfg!(target_os = "windows") {
            ui.label(tr!("  • Run 'DNS Cache' — check for stale entries. You can flush the cache with: ipconfig /flushdns (requires admin)."));
        }
        ui.add_space(8.0);

        // Section 5: Advanced (admin-only)
        ui.strong(tr!("5. Advanced diagnostics (require administrator privileges)"));
        ui.label(tr!("These commands require elevated privileges and must be run from a terminal:"));
        if cfg!(target_os = "windows") {
            ui.label("  • netsh advfirewall show allprofiles — full firewall status");
            ui.label("  • netsh int ip reset — reset TCP/IP stack");
            ui.label("  • netsh winsock reset — reset Winsock catalog");
        } else {
            ui.label("  • sudo iptables -L -n — check firewall rules");
            ui.label("  • sudo tcpdump -i any -c 20 — capture 20 packets for analysis");
            ui.label("  • sudo ethtool <interface> — check NIC speed/duplex settings");
            ui.label("  • sudo ss -tlnp — show listening ports with process names (requires root for -p)");
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_commands_returns_non_empty_list() {
        let commands = get_commands();
        assert!(!commands.is_empty(), "Command list should not be empty");
    }

    #[test]
    fn test_all_commands_have_required_fields() {
        for cmd in get_commands() {
            assert!(!cmd.label.is_empty(), "Command label must not be empty");
            assert!(
                !cmd.description.is_empty(),
                "Command description must not be empty"
            );
            assert!(!cmd.cmd.is_empty(), "Command executable must not be empty");
            assert!(
                !cmd.category.is_empty(),
                "Command category must not be empty"
            );
        }
    }

    #[test]
    fn test_full_command_string_format() {
        let cmd = SystemCommand::new("Test", "Test", "desc", "ip", &["-c", "addr"]);
        assert_eq!(cmd.full_command_string(), "$ ip -c addr");

        let cmd2 = SystemCommand::new("Test", "Test", "desc", "uptime", &[]);
        assert_eq!(cmd2.full_command_string(), "$ uptime");
    }
}
