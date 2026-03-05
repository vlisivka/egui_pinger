use crate::constants::RTT_WARNING_THRESHOLD_MS;
use crate::model::{HostInfo, HostStatus};
use eframe::egui;
use eframe::egui::{Color32, RichText};
use egui_plot::{Bar, BarChart, HLine, Plot};
use tr::tr;

#[allow(clippy::too_many_arguments)]
pub fn render_host_row(
    ui: &mut egui::Ui,
    visuals: &crate::app::PingVisuals,
    host_info: &HostInfo,
    status: &HostStatus,
    idx: usize,
    deleting_host: &mut Option<String>,
    editing_host: &mut Option<String>,
    viewing_route: &mut Option<String>,
    viewing_log: &mut Option<String>,
    toggled_stop: &mut Option<usize>,
    moved: &mut Option<(usize, usize)>,
) {
    let color = visuals.status_color(host_info.is_stopped, status.alive, status.latency);

    let mut parts = Vec::new();
    if host_info.display.show_name {
        parts.push(host_info.name.clone());
    }
    if host_info.display.show_address {
        parts.push(host_info.address.clone());
    }
    parts.push("→".to_string());

    if host_info.display.show_latency {
        if host_info.is_stopped {
            parts.push(tr!("STOPPED").to_string());
        } else if status.alive {
            parts.push(format!("{:4.0}{}", status.latency, tr!("ms")));
        } else {
            let down_text = if let Some(ref fp) = status.failure_point {
                let translated_fp = if fp == "Local Interface" {
                    tr!("Local Interface").to_string()
                } else {
                    fp.clone()
                };
                tr!("DOWN (at {node})").replace("{node}", &translated_fp)
            } else {
                tr!("DOWN").to_string()
            };
            parts.push(format!("{:>4}", down_text));
        }
    }

    struct StatDisplay {
        text: String,
        tooltip: String,
        color: Option<Color32>,
    }
    let mut stats = Vec::new();

    if !host_info.is_stopped {
        let loss_pct =
            (status.lost as f64 / if status.sent == 0 { 1 } else { status.sent } as f64) * 100.0;

        if host_info.display.show_mean {
            stats.push(StatDisplay {
                text: format!("{}: {:4.1}", tr!("M"), status.mean),
                tooltip: tr!("Mean RTT").to_string(),
                color: visuals.value_color(status.mean, 150.0, 300.0, false),
            });
        }
        if host_info.display.show_median {
            stats.push(StatDisplay {
                text: format!("{}: {:4.1}", tr!("Med"), status.median),
                tooltip: tr!("Median RTT").to_string(),
                color: visuals.value_color(status.median, 150.0, 300.0, false),
            });
        }
        if host_info.display.show_rtp_jitter {
            stats.push(StatDisplay {
                text: format!("{}: {:4.1}", tr!("J"), status.rtp_jitter),
                tooltip: tr!("RTP Jitter").to_string(),
                color: visuals.value_color(status.rtp_jitter, 20.0, 30.0, false),
            });
        }
        if host_info.display.show_rtp_mean_jitter {
            stats.push(StatDisplay {
                text: format!("{}: {:4.1}", tr!("Jm"), status.rtp_jitter_mean),
                tooltip: tr!("Mean Jitter").to_string(),
                color: visuals.value_color(status.rtp_jitter_mean, 20.0, 30.0, false),
            });
        }
        if host_info.display.show_rtp_median_jitter {
            stats.push(StatDisplay {
                text: format!("{}: {:4.1}", tr!("Jmed"), status.rtp_jitter_median),
                tooltip: tr!("Median Jitter").to_string(),
                color: visuals.value_color(status.rtp_jitter_median, 20.0, 30.0, false),
            });
        }
        if host_info.display.show_mos {
            stats.push(StatDisplay {
                text: format!("{}: {:3.1}", tr!("MOS"), status.mos),
                tooltip: tr!("Voice Quality (MOS)").to_string(),
                color: visuals.value_color(status.mos, 4.0, 3.6, true),
            });
        }
        if host_info.display.show_availability {
            stats.push(StatDisplay {
                text: format!("{}: {:3.0}%", tr!("Av"), status.availability),
                tooltip: tr!("Availability").to_string(),
                color: visuals.value_color(status.availability, 99.0, 95.0, true),
            });
        }
        if host_info.display.show_outliers {
            stats.push(StatDisplay {
                text: format!("{}: {}", tr!("Out"), status.outliers),
                tooltip: tr!("Outliers (Lags)").to_string(),
                color: if status.outliers > 3 {
                    Some(Color32::from_rgb(230, 159, 0))
                } else {
                    None
                },
            });
        }
        if host_info.display.show_streak {
            let streak_type = if status.streak_success {
                tr!("S")
            } else {
                tr!("F")
            };
            let c = if !status.streak_success && status.streak > 3 {
                Some(Color32::from_rgb(213, 94, 0))
            } else if !status.streak_success && status.streak > 1 {
                Some(if visuals.is_dark {
                    Color32::from_rgb(240, 228, 66)
                } else {
                    Color32::from_rgb(230, 159, 0)
                })
            } else {
                None
            };
            stats.push(StatDisplay {
                text: format!("{}: {}{}", tr!("Str"), streak_type, status.streak),
                tooltip: tr!("Streak").to_string(),
                color: c,
            });
        }
        if host_info.display.show_stddev {
            stats.push(StatDisplay {
                text: format!("{}: {:4.1}", tr!("SD"), status.stddev),
                tooltip: tr!("Standard Deviation").to_string(),
                color: None,
            });
        }
        if host_info.display.show_p95 {
            stats.push(StatDisplay {
                text: format!("95%: {:4.1}", status.p95),
                tooltip: tr!("95th Percentile").to_string(),
                color: visuals.value_color(status.p95, 150.0, 300.0, false),
            });
        }
        if host_info.display.show_min_max {
            stats.push(StatDisplay {
                text: format!(
                    "{}: {:1.0}-{:1.0}",
                    tr!("m/M"),
                    status.min_rtt,
                    status.max_rtt
                ),
                tooltip: tr!("Min / Max RTT").to_string(),
                color: None,
            });
        }
        if host_info.display.show_loss {
            stats.push(StatDisplay {
                text: format!(
                    "{}: {}/{} {:.1}%",
                    tr!("L"),
                    status.lost,
                    status.sent,
                    loss_pct
                ),
                tooltip: tr!("Packet Loss").to_string(),
                color: visuals.value_color(loss_pct, 1.0, 3.0, false),
            });
        }
    }

    let row_id = egui::Id::new("host_row").with(&host_info.address);
    let (inner_res, dropped_payload) = ui.dnd_drop_zone::<usize, ()>(egui::Frame::NONE, |ui| {
        ui.horizontal(|ui| {
            // Drag handle
            let handle_id = row_id.with("handle");
            let handle_res = ui.dnd_drag_source(handle_id, idx, |ui| {
                ui.label(RichText::new(" ☰ ").monospace().strong());
            });
            if handle_res.response.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
            }

            // Host control buttons (positioned left for layout stability)
            if ui.button("x").clicked() {
                *deleting_host = Some(host_info.address.clone());
            }
            if ui.button("⚙").clicked() {
                *editing_host = Some(host_info.address.clone());
            }
            if ui.button("📍").clicked() {
                *viewing_route = Some(host_info.address.clone());
            }
            if ui.button("📋").on_hover_text(tr!("View Log")).clicked() {
                *viewing_log = Some(host_info.address.clone());
            }

            let stop_icon = if host_info.is_stopped { "▶" } else { "⏹" };
            let stop_tooltip = if host_info.is_stopped {
                tr!("Start monitoring")
            } else {
                tr!("Stop monitoring")
            };
            if ui.button(stop_icon).on_hover_text(stop_tooltip).clicked() {
                *toggled_stop = Some(idx);
            }

            // Chart: thin bars — blue (<150ms), yellow/orange (>150ms),
            // and vermilion (timeout) colors
            let chart = BarChart::new(
                String::new(),
                status
                    .history
                    .iter()
                    .enumerate()
                    .map(|(i, &rtt)| {
                        // For timeouts, display bar at warning threshold height
                        let height = if rtt.is_nan() {
                            RTT_WARNING_THRESHOLD_MS
                        } else {
                            rtt
                        };
                        let fill = visuals.latency_color(rtt);

                        Bar::new(i as f64, height).width(1.0).fill(fill)
                    })
                    .collect(),
            )
            .allow_hover(false); // Disable built-in bar tooltips

            // Ping history chart.
            // To fill 300 bars of width 1.0 without gaps:
            // 1. Set X bounds from -0.5 to 299.5 (300 units total).
            // 2. Remove horizontal padding (margin_fraction).
            let plot_res = Plot::new(format!("plot_{}", &host_info.address))
                .height(30.0)
                .width(300.0)
                .show_axes(false)
                .show_grid(false)
                .show_x(false) // Disable built-in tooltip system
                .show_y(false)
                .allow_zoom(false)
                .allow_drag(false)
                .allow_scroll(false)
                .set_margin_fraction(egui::Vec2::new(0.0, 0.05))
                .include_x(-0.5)
                .include_x(299.5)
                .include_y(0.0)
                .include_y(RTT_WARNING_THRESHOLD_MS)
                .show(ui, |plot_ui: &mut egui_plot::PlotUi| {
                    plot_ui.hline(
                        HLine::new("", RTT_WARNING_THRESHOLD_MS)
                            .color(visuals.limit_line_color())
                            .width(1.0),
                    );
                    plot_ui.bar_chart(chart);
                });

            plot_res.response.on_hover_ui(|ui| {
                if let Some(hover_pos) = ui.ctx().pointer_hover_pos() {
                    let pos = plot_res.transform.value_from_position(hover_pos);
                    let i = pos.x.round() as i32;
                    if i >= 0 && i < status.history.len() as i32 {
                        let rtt = status.history[i as usize];
                        let text = if rtt.is_nan() {
                            tr!("Timeout").to_string()
                        } else {
                            format!("{:.1} {}", rtt, tr!("ms"))
                        };
                        ui.horizontal(|ui| {
                            ui.add_space(4.0);
                            ui.label(text);
                            ui.add_space(4.0);
                        });
                    }
                }
            });

            // Label with host name, address, and current latency
            ui.colored_label(
                color,
                RichText::new(format!("{}  ", parts.join(" ")))
                    .monospace()
                    .strong(),
            );

            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                for (i, stat) in stats.iter().enumerate() {
                    let c = stat.color.unwrap_or(color);
                    ui.colored_label(c, RichText::new(&stat.text).monospace().strong())
                        .on_hover_text(&stat.tooltip);

                    if i < stats.len() - 1 {
                        ui.colored_label(color, RichText::new(", ").monospace().strong());
                    }
                }
            });
        });
    });

    let response = inner_res.response;

    // If another row was dropped onto this row
    if let Some(from_idx) = dropped_payload {
        *moved = Some((*from_idx, idx));
    }

    // Highlight on hover during drag-and-drop
    if response.dnd_hover_payload::<usize>().is_some() {
        ui.painter()
            .rect_filled(response.rect, 2.0, Color32::from_white_alpha(30));
    }
}
