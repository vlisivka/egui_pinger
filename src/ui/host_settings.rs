use crate::model::{HostInfo, PingMode};
use eframe::egui;
use tr::tr;

/// Renders the host settings window for adding or editing targets.
///
/// Returns `true` if the help button was clicked inside the window.
pub fn render_host_settings_window(
    ctx: &egui::Context,
    hosts: &mut [HostInfo],
    editing_host: &mut Option<String>,
) -> bool {
    let mut help_requested = false;

    let Some(addr) = editing_host.as_ref() else {
        return false;
    };

    let Some(h) = hosts.iter_mut().find(|h| h.address == *addr) else {
        *editing_host = None;
        return false;
    };

    let mut is_open = true;
    let window_res = egui::Window::new(tr!("Host Settings"))
        .open(&mut is_open)
        .resizable(false)
        .show(ctx, |ui| {
            ui.heading(format!("{}: {}", tr!("Host"), h.address));
            ui.add_space(8.0);

            ui.horizontal(|ui| {
                ui.label(format!("{}:", tr!("Name")));
                ui.text_edit_singleline(&mut h.name);
            });

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.label(tr!("Ping Interval:"));
                egui::ComboBox::from_id_salt(format!("combo_{}", &h.address))
                    .selected_text(h.mode.label())
                    .show_ui(ui, |ui| {
                        for mode in [
                            PingMode::VeryFast,
                            PingMode::Fast,
                            PingMode::NotFast,
                            PingMode::Normal,
                            PingMode::NotSlow,
                            PingMode::Slow,
                            PingMode::VerySlow,
                        ] {
                            ui.selectable_value(&mut h.mode, mode, mode.label());
                        }
                    });
            });

            ui.add_space(8.0);
            ui.label(tr!("VPN & Privacy:"));
            ui.horizontal(|ui| {
                ui.label(tr!("Packet Size:"));
                ui.add(
                    egui::DragValue::new(&mut h.packet_size)
                        .range(16..=1400)
                        .suffix(tr!(" bytes")),
                );
            });
            ui.checkbox(&mut h.random_padding, tr!("Random Padding"))
                .on_hover_text(tr!(
                    "Adds 0-25% random extra data to each packet to mask traffic patterns"
                ));

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.label(tr!("Show fields:"));
                if ui
                    .button(" (?) ")
                    .on_hover_text(tr!("Learn more about these metrics"))
                    .clicked()
                {
                    help_requested = true;
                }
            });
            ui.checkbox(&mut h.display.show_name, tr!("Host Name"))
                .on_hover_text(tr!("User-defined name for this host"));
            ui.checkbox(&mut h.display.show_address, tr!("Host Address"))
                .on_hover_text(tr!("IP address or domain name"));
            ui.checkbox(&mut h.display.show_latency, tr!("Current Latency"))
                .on_hover_text(tr!("Round-trip time of the last packet"));
            ui.checkbox(&mut h.display.show_mean, tr!("Mean RTT"))
                .on_hover_text(tr!("Average latency (can be skewed by spikes)"));
            ui.checkbox(&mut h.display.show_median, tr!("Median RTT"))
                .on_hover_text(tr!("Typical latency (ignores rare spikes)"));
            ui.checkbox(&mut h.display.show_rtp_jitter, tr!("RTP Jitter"))
                .on_hover_text(tr!("Current variation in delay (RFC 3550)"));
            ui.checkbox(&mut h.display.show_rtp_mean_jitter, tr!("Mean Jitter"))
                .on_hover_text(tr!("Average variation over time"));
            ui.checkbox(&mut h.display.show_rtp_median_jitter, tr!("Median Jitter"))
                .on_hover_text(tr!("Typical variation over time"));
            ui.checkbox(&mut h.display.show_mos, tr!("MOS"))
                .on_hover_text(tr!("Voice Quality Score (1.0 = Bad, 4.5 = Excellent)"));
            ui.checkbox(&mut h.display.show_availability, tr!("Availability"))
                .on_hover_text(tr!("Percentage of packets successfully delivered"));
            ui.checkbox(&mut h.display.show_outliers, tr!("Outliers"))
                .on_hover_text(tr!("Count of extremely delayed packets (lags)"));
            ui.checkbox(&mut h.display.show_streak, tr!("Streak"))
                .on_hover_text(tr!("Current consecutive successes or failures"));
            ui.checkbox(&mut h.display.show_stddev, tr!("StdDev"))
                .on_hover_text(tr!("Standard Deviation (spread of latency values)"));
            ui.checkbox(&mut h.display.show_p95, tr!("95th Percentile"))
                .on_hover_text(tr!("Latency experienced by 95% of packets (worst-case)"));
            ui.checkbox(&mut h.display.show_min_max, tr!("Min / Max RTT"))
                .on_hover_text(tr!("Absolute best and worst latency in history"));
            ui.checkbox(&mut h.display.show_loss, tr!("Packet Loss"))
                .on_hover_text(tr!("Count and percentage of dropped packets"));

            ui.add_space(12.0);
            ui.button(tr!("Close")).clicked()
        });

    if let Some(inner_res) = window_res
        && inner_res.inner == Some(true)
    {
        is_open = false;
    }

    // Close window if requested
    if !is_open {
        *editing_host = None;
    }

    help_requested
}
