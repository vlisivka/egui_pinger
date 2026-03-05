use crate::app::PingVisuals;
use crate::constants::MAX_UI_EVENTS;
use crate::model::{AppState, LogEntry};
use eframe::egui;
use eframe::egui::{Color32, RichText};
use std::io::Write;
use tr::tr;

/// Returns the appropriate display color for a log entry based on its type.
pub fn log_entry_color(entry: &LogEntry, visuals: &PingVisuals) -> Color32 {
    match entry {
        LogEntry::Ping { rtt: None, .. } => Color32::from_rgb(230, 159, 0), // Orange (timeout)
        LogEntry::Incident { is_break: true, .. } => Color32::from_rgb(213, 94, 0), // Vermilion (loss)
        LogEntry::Incident {
            is_break: false, ..
        } => Color32::from_rgb(0, 158, 115), // Bluish green
        LogEntry::Statistics { .. } => Color32::from_rgb(0, 158, 115),              // Bluish green
        LogEntry::RouteUpdate { .. } => Color32::from_rgb(0, 114, 178),             // Blue
        LogEntry::Marker { .. } => Color32::from_rgb(204, 121, 167), // Reddish purple
        _ => visuals.latency_color(0.1),                             // Normal ping
    }
}

/// Renders the log viewer window for a specific host.
pub fn render_log_window(
    ctx: &egui::Context,
    visuals: &PingVisuals,
    state: &mut AppState,
    viewing_log: &mut Option<String>,
) {
    let Some(ref addr) = viewing_log.clone() else {
        return;
    };

    // Initialize default log path if empty
    if let Some(h) = state.hosts.iter_mut().find(|h| h.address == *addr)
        && h.log_file_path.is_empty()
    {
        let safe_addr = h.address.replace(['.', ':', '/', '[', ']'], "_");
        if let Some(home) = dirs::home_dir() {
            h.log_file_path = home
                .join(format!("{}.log", safe_addr))
                .to_string_lossy()
                .to_string();
        }
    }

    let mut open = true;
    egui::Window::new(format!("{} - {}", tr!("Log"), addr))
        .open(&mut open)
        .resizable(true)
        .default_width(600.0)
        .default_height(400.0)
        .show(ctx, |ui| {
            // 1. Logging Controls
            ui.horizontal(|ui| {
                // Get data and handle checkbox (ends borrow of h early)
                let (changed, log_file_path, is_active) =
                    if let Some(h) = state.hosts.iter_mut().find(|h| h.address == *addr) {
                        let res = ui.checkbox(&mut h.log_to_file, tr!("Append log to file"));
                        (res.changed(), h.log_file_path.clone(), h.log_to_file)
                    } else {
                        (false, String::new(), false)
                    };

                if changed && !log_file_path.is_empty() {
                    let ts = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
                    let msg = if is_active {
                        tr!("Journal started at")
                    } else {
                        tr!("Journal ended at")
                    };

                    // Write to file
                    if let Ok(mut file) = std::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(&log_file_path)
                    {
                        let _ = writeln!(file, "=== {}: {} ===", msg, ts);
                    }

                    // Write to internal log (for viewer)
                    if let Some(status) = state.statuses.get_mut(addr) {
                        status.events.push_back(LogEntry::Marker {
                            timestamp: chrono::Utc::now().timestamp() as u64,
                            message: msg.to_string(),
                        });
                    }
                }

                // Re-borrow for the text edit
                if let Some(h) = state.hosts.iter_mut().find(|h| h.address == *addr) {
                    ui.add_enabled(
                        h.log_to_file,
                        egui::TextEdit::singleline(&mut h.log_file_path)
                            .hint_text(tr!("Log file path"))
                            .desired_width(300.0),
                    );
                }
            });

            ui.separator();

            // 2. Filters
            ui.horizontal(|ui| {
                ui.label(format!("{}:", tr!("Filters")));
                ui.checkbox(&mut state.log_filter.show_pings, tr!("Pings"));
                ui.checkbox(&mut state.log_filter.show_timeouts, tr!("Timeouts"));
                ui.checkbox(&mut state.log_filter.show_stats, tr!("Statistics"));
                ui.checkbox(&mut state.log_filter.show_route, tr!("Traceroute"));
                ui.checkbox(&mut state.log_filter.show_incidents, tr!("Incidents"));
            });

            ui.separator();

            // 3. The Log View
            if let Some(status) = state.statuses.get(addr) {
                let row_height = ui.text_style_height(&egui::TextStyle::Monospace);
                let total_events = status.events.len();

                // Limit UI view to the last MAX_UI_EVENTS events for performance.
                let start_idx = total_events.saturating_sub(MAX_UI_EVENTS);

                let all_on = state.log_filter.show_pings
                    && state.log_filter.show_timeouts
                    && state.log_filter.show_stats
                    && state.log_filter.show_route
                    && state.log_filter.show_incidents;

                let display_settings = state
                    .hosts
                    .iter()
                    .find(|h| h.address == *addr)
                    .map(|h| h.display.clone());

                if all_on {
                    let view_count = total_events - start_idx;
                    egui::ScrollArea::vertical()
                        .stick_to_bottom(true)
                        .show_rows(ui, row_height, view_count, |ui, range| {
                            for i in range {
                                if let Some(entry) = status.events.get(start_idx + i) {
                                    let text = entry.format(addr, display_settings.as_ref());
                                    let color = log_entry_color(entry, visuals);
                                    ui.label(RichText::new(text).monospace().color(color));
                                }
                            }
                        });
                } else {
                    // Filtering active: collect only matching entries from the recent range
                    let filtered: Vec<&LogEntry> = status
                        .events
                        .range(start_idx..)
                        .filter(|e| match e {
                            LogEntry::Ping { rtt, .. } => {
                                if rtt.is_some() {
                                    state.log_filter.show_pings
                                } else {
                                    state.log_filter.show_timeouts
                                }
                            }
                            LogEntry::Statistics { .. } => state.log_filter.show_stats,
                            LogEntry::RouteUpdate { .. } => state.log_filter.show_route,
                            LogEntry::Incident { .. } => state.log_filter.show_incidents,
                            LogEntry::Marker { .. } => true,
                        })
                        .collect();

                    egui::ScrollArea::vertical()
                        .stick_to_bottom(true)
                        .show_rows(ui, row_height, filtered.len(), |ui, range| {
                            for i in range {
                                let entry = filtered[i];
                                let text = entry.format(addr, display_settings.as_ref());
                                let color = log_entry_color(entry, visuals);
                                ui.label(RichText::new(text).monospace().color(color));
                            }
                        });
                }
            }
        });
    if !open {
        *viewing_log = None;
    }
}
