use crate::app::PingVisuals;
use crate::model::HostStatus;
use eframe::egui;
use eframe::egui::{Color32, RichText};
use std::collections::HashMap;
use tr::tr;

/// Renders the traceroute path viewer window for a specific host.
pub fn render_route_window(
    ctx: &egui::Context,
    visuals: &PingVisuals,
    statuses: &mut HashMap<String, HostStatus>,
    viewing_route: &mut Option<String>,
) {
    let Some(ref addr) = viewing_route.clone() else {
        return;
    };
    let mut open = true;
    egui::Window::new(format!("{} - {}", tr!("Route"), addr))
        .open(&mut open)
        .resizable(true)
        .default_width(400.0)
        .show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                // Mutable borrow for refresh button
                if let Some(status) = statuses.get_mut(addr) {
                    ui.horizontal(|ui| {
                        ui.heading(tr!("Path to target:"));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("🔄").on_hover_text(tr!("Refresh")).clicked() {
                                status.manual_trace_requested = true;
                            }
                        });
                    });
                }

                // Immutable borrow for display
                if let Some(status) = statuses.get(addr) {
                    if status.traceroute_path.is_empty() {
                        ui.label(tr!("Discovering route..."));
                    } else {
                        for (i, hop_addr) in status.traceroute_path.iter().enumerate() {
                            let hop_status = statuses.get(hop_addr);
                            ui.horizontal(|ui| {
                                ui.label(format!("{}.", i + 1));
                                ui.monospace(hop_addr);
                                if let Some(hs) = hop_status {
                                    if hs.alive {
                                        ui.label(
                                            RichText::new(format!("({:.1} ms)", hs.latency))
                                                .color(visuals.latency_color(hs.latency)),
                                        );
                                    } else {
                                        ui.label(
                                            RichText::new(tr!("(TIMEOUT)"))
                                                .color(Color32::from_rgb(213, 94, 0)),
                                        );
                                    }
                                }
                            });
                        }
                    }

                    if status.tracer_in_progress {
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(tr!("Refreshing route..."));
                        });
                    }
                } else {
                    ui.label(tr!("No status found for this address."));
                }
            });
        });
    if !open {
        *viewing_route = None;
    }
}
