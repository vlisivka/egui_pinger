use crate::app::HelpTab;
use eframe::egui;
use tr::tr;

/// Renders the help window explaining network statistics metrics.
pub fn render_help_window(ctx: &egui::Context, help_open: &mut bool, selected_tab: &mut HelpTab) {
    let mut open_var = true;
    let window_res = egui::Window::new(tr!("Network Statistics Information"))
        .open(&mut open_var)
        .resizable(true)
        .default_width(450.0)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(selected_tab, HelpTab::Latency, tr!("Latency"));
                ui.selectable_value(selected_tab, HelpTab::Jitter, tr!("Jitter"));
                ui.selectable_value(selected_tab, HelpTab::Quality, tr!("Quality & MOS"));
                ui.selectable_value(selected_tab, HelpTab::Reliability, tr!("Reliability"));
                ui.selectable_value(selected_tab, HelpTab::Internet, tr!("Internet Check"));
            });
            ui.separator();
            egui::ScrollArea::vertical().show(ui, |ui| {
                match selected_tab {
                    HelpTab::Latency => {
                        ui.strong(tr!("Round-Trip Time (RTT) - What is Latency?"));
                        ui.label(tr!("Latency (RTT) is the total time it takes for a signal to go from your computer to the server and back. In network diagnostics, this is the most basic measure of 'speed'."));
                        ui.add_space(8.0);

                        ui.strong(tr!("How it is calculated:"));
                        ui.label(tr!("- Mean (Average): The sum of all RTTs divided by the number of packets. Good for general trends, but can be misleading if you have rare, massive 'lags'."));
                        ui.label(tr!("- Median (Middle Value): We sort all results and pick the one in the middle. This is the 'typical' experience. If you have 100 packets and 1 of them is very slow, the Median stays the same, while the Mean jumps up."));
                        ui.label(tr!("- 95th Percentile (P95): This shows the worst-case scenario for 95% of your traffic. If P95 is low, your connection is stable. If it's much higher than the Median, your connection is 'jittery' and prone to sudden lags."));

                        ui.add_space(8.0);
                        ui.strong(tr!("VoIP Impact:"));
                        ui.label(tr!("Voice is a real-time stream. If latency is over 150ms, you start to notice a delay in the conversation (waiting for the other person to respond). Above 300ms, people will start 'talking over' each other because of the lag."));
                    },
                    HelpTab::Jitter => {
                        ui.strong(tr!("Jitter - Stability of the Connection"));
                        ui.label(tr!("Jitter is the 'shaking' of your latency. It measures how much the delay between packets changes over time."));
                        ui.add_space(8.0);

                        ui.strong(tr!("How it is calculated:"));
                        ui.label(tr!("We use the RFC 3550 algorithm (Standard for RTP). It doesn't just look at the highest and lowest values; it calculates the difference between consecutive packets and applies a smoothing filter."));
                        ui.label(tr!("Formally: J = J + (|D| - J) / 16, where D is the difference between the current and previous packet delay. This provides a stable 'moving average' of network stability."));

                        ui.add_space(8.0);
                        ui.strong(tr!("VoIP Impact:"));
                        ui.label(tr!("Phones expect audio packets to arrive in a steady 'heartbeat' (every 20ms). If Jitter is high (>30ms), packets arrive in 'clumps' or too late to be played. This causes the voice to sound 'robotic', 'choppy', or broken."));
                    },
                    HelpTab::Quality => {
                        ui.strong(tr!("MOS - The 'Voice Score'"));
                        ui.label(tr!("MOS (Mean Opinion Score) is a 1.0 to 4.5 rating that predicts how a human would rate the call quality."));
                        ui.add_space(8.0);

                        ui.strong(tr!("How we calculate it:"));
                        ui.label(tr!("We implement a simplified ITU-T G.107 'E-model'. It takes your current Latency, Jitter, and Packet Loss, and calculates an 'R-factor'. This factor is then mapped to the MOS scale."));
                        ui.add_space(4.0);
                        ui.label(tr!("- 4.3 - 4.5 (Excellent): Crystal clear HD audio, like sitting in the same room."));
                        ui.label(tr!("- 4.0 - 4.2 (Good): Standard clean call. No issues."));
                        ui.label(tr!("- 3.6 - 3.9 (Fair): You can hear 'compression' or minor clicks. Acceptable for business."));
                        ui.label(tr!("- Below 3.0 (Poor): Words are missing, voice is distorted. It's time to hang up and check your router."));
                    },
                    HelpTab::Reliability => {
                        ui.strong(tr!("Reliability - Packet Loss & Outliers"));
                        ui.label(tr!("This tab tracks if packets are actually reaching their destination and if any are 'statistical anomalies'."));
                        ui.add_space(8.0);

                        ui.strong(tr!("Definitions:"));
                        ui.label(tr!("- Packet Loss: The most critical metric. If a packet is lost, a piece of someone's word is gone forever. VoIP cannot 'redownload' lost audio like a file transfer does."));
                        ui.label(tr!("- Outliers: These are packets that didn't go missing but took much longer than usual (more than 3 standard deviations from the mean). In a call, these cause a temporary 'freeze' or a loud 'pop' in the audio."));
                        ui.label(tr!("- Streak: Shows how many times in a row a host has responded (S) or failed (F). Long 'F' streaks mean the connection is completely down."));

                        ui.add_space(8.0);
                        ui.strong(tr!("VoIP Impact:"));
                        ui.label(tr!("While 1% loss might be okay for browsing, for VoIP it means every 100th piece of a word is missing. Above 2-3% loss, the conversation becomes extremely difficult to understand."));
                    },
                    HelpTab::Internet => {
                        ui.strong(tr!("Reliable Hosts for Internet Checks"));
                        ui.label(tr!("If you want to check if YOUR internet is working (rather than a specific service), use these stable public DNS servers:"));
                        ui.add_space(8.0);

                        ui.label(tr!("- Google DNS: 8.8.8.8 or 8.8.4.4"));
                        ui.label(tr!("- Cloudflare: 1.1.1.1 or 1.0.0.1"));
                        ui.label(tr!("- Quad9: 9.9.9.9"));
                        ui.add_space(8.0);

                        ui.strong(tr!("Tip:"));
                        ui.label(tr!("If you can ping 8.8.8.8 but cannot open 'google.com', you likely have a DNS problem, not a connection problem."));
                    }
                }
            });
            ui.add_space(8.0);
            ui.button(tr!("Close")).clicked()
        });
    if !open_var || (window_res.is_some() && window_res.unwrap().inner == Some(true)) {
        *help_open = false;
    }
}
