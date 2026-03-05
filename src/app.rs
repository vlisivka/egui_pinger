use crate::logic::{SharedState, pinger_task};
use crate::model::{AppState, DisplaySettings, HostInfo, HostStatus, PingMode};
use crate::ui::system_tools::{SystemToolsState, ui_system_tools_window};
use eframe::egui;
use eframe::egui::Color32;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tr::tr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HelpTab {
    #[default]
    Latency,
    Jitter,
    Quality,
    Reliability,
    Internet,
}

/// The main application state and UI controller.
///
/// This struct holds the shared state (list of hosts and their statuses)
/// as well as the UI state (modal window flags, input fields, etc.).
pub struct EguiPinger {
    pub(crate) state: SharedState,
    pub input_name: String,
    pub input_address: String,
    pub(crate) editing_host: Option<String>,
    pub(crate) deleting_host: Option<String>,
    pub(crate) help_window_open: bool,
    pub(crate) selected_help_tab: HelpTab,
    pub(crate) viewing_route: Option<String>,
    pub viewing_log: Option<String>,
    pub(crate) system_tools_open: bool,
    pub(crate) system_tools: SystemToolsState,
}

/// Helper for application-specific colors adapted for light/dark themes.
/// Encapsulates visual styling rules for the dashboard.
///
/// Uses theme-aware (light/dark) color palettes to represent latency ranges,
/// status changes, and grid styles.
pub struct PingVisuals {
    pub is_dark: bool,
}

impl PingVisuals {
    /// Creates a new `PingVisuals` based on the current UI theme.
    pub fn from_ctx(ctx: &egui::Context) -> Self {
        Self {
            is_dark: ctx.style().visuals.dark_mode,
        }
    }

    /// Returns the color for limit lines in charts.
    pub fn limit_line_color(&self) -> Color32 {
        if self.is_dark {
            Color32::from_gray(80)
        } else {
            Color32::from_gray(160)
        }
    }

    /// Returns a theme-aware color representing the given latency range.
    pub fn latency_color(&self, rtt: f64) -> Color32 {
        if rtt.is_nan() {
            Color32::from_rgb(213, 94, 0) // Vermilion
        } else if rtt > 300.0 {
            Color32::from_rgb(204, 121, 167) // Reddish purple
        } else if rtt > 150.0 {
            if self.is_dark {
                Color32::from_rgb(240, 228, 66) // Yellow
            } else {
                Color32::from_rgb(230, 159, 0) // Orange
            }
        } else if self.is_dark {
            Color32::from_rgb(86, 180, 233) // Sky Blue
        } else {
            Color32::from_rgb(0, 114, 178) // Blue
        }
    }

    /// Returns an optional alert color if a value exceeds thresholds.
    pub fn value_color(
        &self,
        value: f64,
        warn_th: f64,
        bad_th: f64,
        higher_is_better: bool,
    ) -> Option<Color32> {
        if value.is_nan() {
            return None;
        }
        let is_bad = if higher_is_better {
            value < bad_th
        } else {
            value > bad_th
        };
        let is_warn = if higher_is_better {
            value < warn_th
        } else {
            value > warn_th
        };

        let bad_c = Color32::from_rgb(213, 94, 0); // Vermilion
        let warn_c = if self.is_dark {
            Color32::from_rgb(240, 228, 66)
        } else {
            Color32::from_rgb(230, 159, 0)
        };

        if is_bad {
            Some(bad_c)
        } else if is_warn {
            Some(warn_c)
        } else {
            None
        }
    }

    /// Returns a color representing the combined availability and latency state.
    pub fn status_color(&self, is_stopped: bool, alive: bool, latency: f64) -> Color32 {
        if is_stopped {
            if self.is_dark {
                Color32::from_gray(128)
            } else {
                Color32::from_gray(160)
            }
        } else if !alive {
            self.latency_color(f64::NAN)
        } else {
            self.latency_color(latency)
        }
    }
}

impl EguiPinger {
    /// Creates a new application instance, restoring state from storage if available.
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let state = Arc::new(Mutex::new(match cc.storage {
            Some(storage) => {
                if let Some(serialized) = storage.get_string(eframe::APP_KEY) {
                    serde_json::from_str(&serialized).unwrap_or_default()
                } else {
                    AppState::default()
                }
            }
            None => AppState::default(),
        }));

        let state_clone = state.clone();
        std::thread::spawn(move || {
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(pinger_task(state_clone));
        });

        let app = Self {
            state,
            input_name: String::new(),
            input_address: String::new(),
            editing_host: None,
            deleting_host: None,
            help_window_open: false,
            selected_help_tab: HelpTab::default(),
            viewing_route: None,
            viewing_log: None,
            system_tools_open: false,
            system_tools: SystemToolsState::default(),
        };

        // Add startup markers for hosts with logging enabled
        app.add_marker_to_all_active_logs(true);

        app
    }

    fn add_marker_to_all_active_logs(&self, is_start: bool) {
        let mut state = self.state.lock().expect("State mutex poisoned");
        let msg = if is_start {
            tr!("Journal started at")
        } else {
            tr!("Journal ended at")
        };
        let ts = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let now_ts = chrono::Utc::now().timestamp() as u64;

        // Collect info to avoid borrow conflicts
        let targets: Vec<crate::model::HostInfo> = state
            .hosts
            .iter()
            .filter(|h| h.log_to_file && !h.log_file_path.is_empty())
            .cloned()
            .collect();

        for host in targets {
            let addr = host.address.clone();
            // Write to file
            host.append_to_log(&[format!("=== {}: {} ===", msg, ts)]);

            // Write to internal log (for viewer)
            let status = state.statuses.entry(addr.clone()).or_default();
            status.events.push_back(crate::model::LogEntry::Marker {
                timestamp: now_ts,
                message: msg.to_string(),
            });
            status.trim_events();
        }
    }

    pub fn from_state(state: SharedState) -> Self {
        Self {
            state,
            input_name: String::new(),
            input_address: String::new(),
            editing_host: None,
            deleting_host: None,
            help_window_open: false,
            selected_help_tab: HelpTab::default(),
            viewing_route: None,
            viewing_log: None,
            system_tools_open: false,
            system_tools: SystemToolsState::default(),
        }
    }

    /// The main UI decomposition function that orchestrates all sub-windows and the host list.
    pub fn ui_layout(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                egui::ScrollArea::horizontal().show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let name_field_id = ui.make_persistent_id("name_field");
                        let addr_field_id = ui.make_persistent_id("addr_field");

                        let rs1 = ui.add(
                            egui::TextEdit::singleline(&mut self.input_name)
                                .id(name_field_id)
                                .char_limit(256)
                                .hint_text(tr!("Host name"))
                                .desired_width(256.0),
                        );

                        let rs2 = ui.add(
                            egui::TextEdit::singleline(&mut self.input_address)
                                .id(addr_field_id)
                                .char_limit(256)
                                .hint_text(tr!("Host address"))
                                .desired_width(256.0),
                        );

                        // When "Add" button is clicked or Enter is pressed in the second field,
                        // add host to the list
                        if (ui.button(tr!("Add")).clicked()
                            || (rs2.lost_focus()
                                && rs2.ctx.input(|i| i.key_pressed(egui::Key::Enter))))
                            && !self.input_address.trim().is_empty()
                        {
                            let name = self.input_name.trim().to_string();
                            let address = self.input_address.trim().to_lowercase();

                            let mut state = self.state.lock().expect("State mutex poisoned");
                            if !state.hosts.iter().any(|h| h.address == address) {
                                state
                                    .statuses
                                    .insert(address.clone(), HostStatus::default());
                                let mut host_info = HostInfo {
                                    name,
                                    address,
                                    mode: PingMode::NotFast,
                                    display: DisplaySettings::default(),
                                    packet_size: 16,
                                    random_padding: false,
                                    log_to_file: false,
                                    log_file_path: String::new(),
                                    is_stopped: false,
                                };
                                if host_info.is_local() {
                                    host_info.mode = PingMode::Fast;
                                }
                                state.hosts.push(host_info);
                            }

                            self.input_name.clear();
                            self.input_address.clear();

                            ui.memory_mut(|mem| mem.request_focus(name_field_id));
                        }

                        // When Enter is pressed in the first field, move focus to the second field
                        if rs1.lost_focus() && rs1.ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
                            ui.memory_mut(|mem| mem.request_focus(addr_field_id));
                        }

                        // Theme toggle and tools button (right-aligned)
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let mut theme = ui.ctx().options(|o| o.theme_preference);
                            let old_theme = theme;
                            theme.radio_buttons(ui);
                            if theme != old_theme {
                                ui.ctx().options_mut(|o| o.theme_preference = theme);
                            }
                            if ui.button("🔧").on_hover_text(tr!("System Tools")).clicked() {
                                self.system_tools_open = !self.system_tools_open;
                            }
                        });
                    });

                    ui.separator();

                    // Clone only the Arc to decouple MutexGuard from self
                    let state_arc = self.state.clone();
                    let visuals = PingVisuals::from_ctx(ctx);
                    let default_host_status = HostStatus::default();
                    let mut moved = None;
                    let mut toggled_stop = None;

                    {
                        let state = state_arc.lock().expect("State mutex poisoned");

                        for (idx, host_info) in state.hosts.iter().enumerate() {
                            let status = state
                                .statuses
                                .get(&host_info.address)
                                .unwrap_or(&default_host_status);

                            crate::ui::host_row::render_host_row(
                                ui,
                                &visuals,
                                host_info,
                                status,
                                idx,
                                &mut self.deleting_host,
                                &mut self.editing_host,
                                &mut self.viewing_route,
                                &mut self.viewing_log,
                                &mut toggled_stop,
                                &mut moved,
                            );
                        }
                    } // End of state MutexGuard scope

                    // Apply reordering
                    if let Some((from, to)) = moved
                        && from != to
                    {
                        let mut state = self.state.lock().expect("State mutex poisoned");
                        let item = state.hosts.remove(from);
                        state.hosts.insert(to, item);
                    }

                    if let Some(idx) = toggled_stop {
                        let mut state = self.state.lock().expect("State mutex poisoned");
                        if let Some(host) = state.hosts.get_mut(idx) {
                            host.is_stopped = !host.is_stopped;
                            let host_is_stopped = host.is_stopped;
                            let msg = if host_is_stopped {
                                tr!("Monitoring stopped")
                            } else {
                                tr!("Monitoring started")
                            };
                            let ts = chrono::Utc::now().timestamp() as u64;
                            let addr = host.address.clone();

                            let file_ts =
                                chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
                            host.append_to_log(&[format!("=== {}: {} ===", msg, file_ts)]);

                            if let Some(status) = state.statuses.get_mut(&addr) {
                                if host_is_stopped {
                                    status.reset_statistics();
                                }
                                status.events.push_back(crate::model::LogEntry::Marker {
                                    timestamp: ts,
                                    message: msg.to_string(),
                                });
                                status.trim_events();
                            }
                        }
                    }

                    // Deletion confirmation dialog
                    if let Some(address) = self.deleting_host.clone() {
                        let name = {
                            let state = self.state.lock().expect("State mutex poisoned");
                            state
                                .hosts
                                .iter()
                                .find(|h| h.address == address)
                                .map(|h| h.name.clone())
                                .unwrap_or_else(|| address.clone())
                        };

                        egui::Window::new(tr!("Confirm Deletion"))
                            .collapsible(false)
                            .resizable(false)
                            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                            .show(ctx, |ui| {
                                ui.label(format!(
                                    "{}: {} ({})?",
                                    tr!("Are you sure you want to remove this host"),
                                    name,
                                    address
                                ));
                                ui.add_space(12.0);
                                ui.horizontal(|ui| {
                                    if ui.button(tr!("Delete")).clicked() {
                                        let mut state =
                                            self.state.lock().expect("State mutex poisoned");
                                        state.hosts.retain(|h| h.address != address);
                                        state.statuses.remove(&address);
                                        self.deleting_host = None;
                                    }
                                    if ui.button(tr!("Cancel")).clicked() {
                                        self.deleting_host = None;
                                    }
                                });
                            });
                    }

                    // Host settings dialog
                    if self.editing_host.is_some() {
                        let mut state = self.state.lock().expect("State mutex poisoned");
                        if crate::ui::host_settings::render_host_settings_window(
                            ctx,
                            &mut state.hosts,
                            &mut self.editing_host,
                        ) {
                            self.help_window_open = true;
                        }
                    }

                    // Traceroute viewer dialog
                    if self.viewing_route.is_some() {
                        let mut state = self.state.lock().expect("State mutex poisoned");
                        crate::ui::route_viewer::render_route_window(
                            ctx,
                            &visuals,
                            &mut state.statuses,
                            &mut self.viewing_route,
                        );
                    }

                    // Help window
                    if self.help_window_open {
                        crate::ui::help::render_help_window(
                            ctx,
                            &mut self.help_window_open,
                            &mut self.selected_help_tab,
                        );
                    }

                    // --- System Tools Window ---
                    if self.system_tools_open {
                        ui_system_tools_window(
                            ctx,
                            &mut self.system_tools_open,
                            &mut self.system_tools,
                        );
                    }

                    // --- Log Window ---
                    if self.viewing_log.is_some() {
                        let mut state = self.state.lock().expect("State mutex poisoned");
                        crate::ui::log_viewer::render_log_window(
                            ctx,
                            &visuals,
                            &mut state,
                            &mut self.viewing_log,
                        );
                    }
                })
            })
        });
    }
}

impl eframe::App for EguiPinger {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        let state = self.state.lock().expect("State mutex poisoned");
        let serialized = serde_json::to_string_pretty(&*state).unwrap_or_default();
        storage.set_string(eframe::APP_KEY, serialized);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.add_marker_to_all_active_logs(false);
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.ui_layout(ctx);
        ctx.request_repaint_after(Duration::from_millis(1000));
    }
}
