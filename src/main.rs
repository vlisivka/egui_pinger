#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release

use eframe::egui;
use eframe::egui::{Color32, RichText};
use egui_plot::{Bar, BarChart, HLine, Plot};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tr::tr;
#[cfg(not(feature = "embed-locales"))]
use tr::tr_init;
mod logic;
mod model;

use logic::{SharedState, pinger_task};
use model::{AppState, DisplaySettings, HostInfo, HostStatus, PingMode};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HelpTab {
    #[default]
    Latency,
    Jitter,
    Quality,
    Reliability,
}

pub struct EguiPinger {
    pub(crate) state: SharedState,
    pub(crate) input_name: String,
    pub(crate) input_address: String,
    pub(crate) editing_host: Option<String>,
    pub(crate) deleting_host: Option<String>,
    pub(crate) help_window_open: bool,
    pub(crate) selected_help_tab: HelpTab,
}

/// Helper for application-specific colors adapted for light/dark themes.
struct PingVisuals {
    pub is_dark: bool,
}

impl PingVisuals {
    fn from_ctx(ctx: &egui::Context) -> Self {
        Self {
            is_dark: ctx.style().visuals.dark_mode,
        }
    }

    fn limit_line_color(&self) -> Color32 {
        if self.is_dark {
            Color32::from_gray(80)
        } else {
            Color32::from_gray(160)
        }
    }

    fn latency_color(&self, rtt: f64) -> Color32 {
        if rtt.is_nan() {
            if self.is_dark {
                Color32::RED
            } else {
                Color32::from_rgb(200, 0, 0)
            }
        } else if rtt > 300.0 {
            if self.is_dark {
                Color32::from_rgb(160, 32, 240)
            } else {
                Color32::from_rgb(120, 0, 200)
            }
        } else if rtt > 150.0 {
            if self.is_dark {
                Color32::YELLOW
            } else {
                Color32::from_rgb(180, 140, 0)
            }
        } else {
            if self.is_dark {
                Color32::from_rgb(0, 255, 100)
            } else {
                Color32::from_rgb(0, 150, 0)
            }
        }
    }

    fn status_color(&self, alive: bool, latency: f64) -> Color32 {
        if !alive {
            self.latency_color(f64::NAN)
        } else {
            self.latency_color(latency)
        }
    }
}

impl EguiPinger {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
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

        Self {
            state,
            input_name: String::new(),
            input_address: String::new(),
            editing_host: None,
            deleting_host: None,
            help_window_open: false,
            selected_help_tab: HelpTab::default(),
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
        }
    }

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
                                .char_limit(20)
                                .hint_text(tr!("Host name"))
                                .desired_width(8.0 * 20.0),
                        );

                        let rs2 = ui.add(
                            egui::TextEdit::singleline(&mut self.input_address)
                                .id(addr_field_id)
                                .char_limit(20)
                                .hint_text(tr!("Host address"))
                                .desired_width(8.0 * 20.0),
                        );

                        // When "Add" button is clicked or Enter is pressed in the second field,
                        // add host to the list
                        if (ui.button(tr!("Add")).clicked()
                            || (rs2.lost_focus()
                                && rs2.ctx.input(|i| i.key_pressed(egui::Key::Enter))))
                            && !self.input_address.trim().is_empty()
                        {
                            let name = self.input_name.trim().to_string();
                            let address = self.input_address.trim().to_string();

                            let mut state = self.state.lock().unwrap();
                            if !state.hosts.iter().any(|h| h.address == address) {
                                state
                                    .statuses
                                    .insert(address.clone(), HostStatus::default());
                                let mut host_info = HostInfo {
                                    name,
                                    address,
                                    mode: PingMode::Slow,
                                    display: DisplaySettings::default(),
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

                        // При натиснені клавіші Enter у першому полі, перемістити фокус на друге поле
                        if rs1.lost_focus() && rs1.ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
                            ui.memory_mut(|mem| mem.request_focus(addr_field_id));
                        }

                        // Перемикач тем (справа)
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let mut theme = ui.ctx().options(|o| o.theme_preference);
                            let old_theme = theme;
                            theme.radio_buttons(ui);
                            if theme != old_theme {
                                ui.ctx().options_mut(|o| o.theme_preference = theme);
                            }
                        });
                    });

                    ui.separator();

                    // Клонуємо потрібні дані один раз — уникаємо тимчасових значень
                    let (hosts, statuses) = {
                        let state = self.state.lock().unwrap();
                        (state.hosts.clone(), state.statuses.clone())
                    };

                    let visuals = PingVisuals::from_ctx(ctx);
                    let default_host_status = HostStatus::default();

                    let mut moved = None;

                    for (idx, host_info) in hosts.iter().enumerate() {
                        let status = statuses
                            .get(&host_info.address)
                            .unwrap_or(&default_host_status);

                        let color = visuals.status_color(status.alive, status.latency);

                        let mut parts = Vec::new();
                        if host_info.display.show_name {
                            parts.push(format!("{:<20}", host_info.name));
                        }
                        if host_info.display.show_address {
                            parts.push(format!("{:<15}", host_info.address));
                        }
                        parts.push("→".to_string());

                        if host_info.display.show_latency {
                            if status.alive {
                                parts.push(format!("{:4.0}ms", status.latency));
                            } else {
                                parts.push(format!("{:>4}", tr!("DOWN")));
                            }
                        }

                        let mut stats = Vec::new();
                        if host_info.display.show_mean {
                            stats.push(format!("{}: {:4.1}", tr!("M"), status.mean));
                        }
                        if host_info.display.show_median {
                            stats.push(format!("{}: {:4.1}", tr!("Med"), status.median));
                        }
                        if host_info.display.show_rtp_jitter {
                            stats.push(format!("{}: {:4.1}", tr!("J"), status.rtp_jitter));
                        }
                        if host_info.display.show_rtp_mean_jitter {
                            stats.push(format!("{}: {:4.1}", tr!("Jm"), status.rtp_jitter_mean));
                        }
                        if host_info.display.show_rtp_median_jitter {
                            stats.push(format!(
                                "{}: {:4.1}",
                                tr!("Jmed"),
                                status.rtp_jitter_median
                            ));
                        }
                        if host_info.display.show_mos {
                            stats.push(format!("{}: {:3.1}", tr!("MOS"), status.mos));
                        }
                        if host_info.display.show_availability {
                            stats.push(format!("{}: {:3.0}%", tr!("Av"), status.availability));
                        }
                        if host_info.display.show_outliers {
                            stats.push(format!("{}: {}", tr!("Out"), status.outliers));
                        }
                        if host_info.display.show_streak {
                            let streak_type = if status.streak_success {
                                tr!("S")
                            } else {
                                tr!("F")
                            };
                            stats.push(format!("{}: {}{}", tr!("Str"), streak_type, status.streak));
                        }
                        if host_info.display.show_stddev {
                            stats.push(format!("{}: {:4.1}", tr!("SD"), status.stddev));
                        }
                        if host_info.display.show_p95 {
                            stats.push(format!("95%: {:4.1}", status.p95));
                        }
                        if host_info.display.show_min_max {
                            stats.push(format!(
                                "{}: {:1.0}-{:1.0}",
                                tr!("m/M"),
                                status.min_rtt,
                                status.max_rtt
                            ));
                        }
                        if host_info.display.show_loss {
                            let loss_pct = (status.lost as f64
                                / if status.sent == 0 { 1 } else { status.sent } as f64)
                                * 100.0;
                            stats.push(format!(
                                "{}: {}/{} {:.2}%",
                                tr!("L"),
                                status.lost,
                                status.sent,
                                loss_pct
                            ));
                        }

                        let text = format!("{} {}", parts.join(" "), stats.join(", "));

                        let row_id = egui::Id::new("host_row").with(&host_info.address);
                        let (inner_res, dropped_payload) =
                            ui.dnd_drop_zone::<usize, ()>(egui::Frame::NONE, |ui| {
                                ui.horizontal(|ui| {
                                    // Ручка для перетягування
                                    let handle_id = row_id.with("handle");
                                    let handle_res = ui.dnd_drag_source(handle_id, idx, |ui| {
                                        ui.label(RichText::new(" ☰ ").monospace().strong());
                                    });
                                    if handle_res.response.hovered() {
                                        ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
                                    }

                                    // Кнопки управління хостом (тепер зліва для стабільності)
                                    if ui.button("x").clicked() {
                                        self.deleting_host = Some(host_info.address.clone());
                                    }
                                    if ui.button("⚙").clicked() {
                                        self.editing_host = Some(host_info.address.clone());
                                    }

                                    // Графік — тоненькі стовпчики зеленого (для <100 мс), жовтого (для >100 мс ),
                                    // і червоного (для пропущених) кольорів
                                    let chart = BarChart::new(
                                        tr!("Pings"),
                                        status
                                            .history
                                            .iter()
                                            .enumerate()
                                            .map(|(i, &rtt)| {
                                                // Якщо пропущений, робимо стовпчик висотою 150 мс
                                                let height = if rtt.is_nan() { 150.0 } else { rtt };
                                                let fill = visuals.latency_color(rtt);

                                                Bar::new(i as f64, height).width(1.0).fill(fill)
                                            })
                                            .collect(),
                                    );

                                    // Графік історії пінгів.
                                    // Щоб 300 стовпчиків шириною 1.0 заповнювали весь простір без "чорних смужок":
                                    // 1. Встановлюємо межі X від -0.5 до 299.5 (разом 300 одиниць).
                                    // 2. Прибираємо горизонтальні відступи (margin_fraction).
                                    Plot::new(format!("plot_{}", &host_info.address))
                                        .height(30.0)
                                        .width(300.0)
                                        .show_axes(false)
                                        .show_grid(false)
                                        .allow_zoom(false)
                                        .allow_drag(false)
                                        .allow_scroll(false)
                                        .set_margin_fraction(egui::Vec2::new(0.0, 0.05))
                                        .include_x(-0.5)
                                        .include_x(299.5)
                                        .include_y(0.0)
                                        .include_y(150.0)
                                        .show(ui, |plot_ui| {
                                            plot_ui.hline(
                                                HLine::new("", 150.0)
                                                    .color(visuals.limit_line_color())
                                                    .width(1.0),
                                            );
                                            plot_ui.bar_chart(chart);
                                        });

                                    // Текст з назвою, адресою, і результатами. Шрифт фіксованої ширини, жирний.
                                    ui.colored_label(
                                        color,
                                        RichText::new(text).monospace().strong(),
                                    );
                                });
                            });

                        let response = inner_res.response;

                        // Якщо на цей рядок скинули інший рядок
                        if let Some(from_idx) = dropped_payload {
                            moved = Some((*from_idx, idx));
                        }

                        // Підсвітка при наведенні під час перетягування
                        if response.dnd_hover_payload::<usize>().is_some() {
                            ui.painter().rect_filled(
                                response.rect,
                                2.0,
                                Color32::from_white_alpha(30),
                            );
                        }
                    }

                    // Виконуємо перестановку
                    if let Some((from, to)) = moved {
                        if from != to {
                            let mut state = self.state.lock().unwrap();
                            let item = state.hosts.remove(from);
                            state.hosts.insert(to, item);
                        }
                    }

                    // Діалог підтвердження видалення
                    if let Some(address) = self.deleting_host.clone() {
                        let name = {
                            let state = self.state.lock().unwrap();
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
                                        let mut state = self.state.lock().unwrap();
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

                    // Діалог налаштувань хоста
                    if let Some(ref addr) = self.editing_host {
                        let mut is_open = true;
                        let mut host_copy = None;

                        {
                            let state = self.state.lock().unwrap();
                            if let Some(h) = state.hosts.iter().find(|h| h.address == *addr) {
                                host_copy = Some(h.clone());
                            }
                        }

                        if let Some(mut h) = host_copy {
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
                                    ui.label(tr!("Ping Interval:"));
                                    ui.radio_value(&mut h.mode, PingMode::Fast, tr!("Fast (1s)"));
                                    ui.radio_value(&mut h.mode, PingMode::Slow, tr!("Slow (1m)"));

                                    ui.add_space(8.0);
                                    ui.horizontal(|ui| {
                                        ui.label(tr!("Show fields:"));
                                        if ui.button(" (?) ").on_hover_text(tr!("Learn more about these metrics")).clicked() {
                                            self.help_window_open = true;
                                        }
                                    });
                                    ui.checkbox(&mut h.display.show_name, tr!("Host Name"))
                                        .on_hover_text(tr!("Show the custom name of the host"));
                                    ui.checkbox(
                                        &mut h.display.show_address,
                                        tr!("Host IP Address"),
                                    ).on_hover_text(tr!("Show the IP address or domain of the host"));
                                    ui.checkbox(
                                        &mut h.display.show_latency,
                                        tr!("Current Latency (Latest RTT)"),
                                    ).on_hover_text(tr!("Show the round-trip time for the last packet"));
                                    ui.checkbox(
                                        &mut h.display.show_mean,
                                        tr!("Mean RTT (Average latency)"),
                                    ).on_hover_text(tr!("Show average latency over the history window"));
                                    ui.checkbox(
                                        &mut h.display.show_median,
                                        tr!("Median RTT (Middle value, robust to spikes)"),
                                    ).on_hover_text(tr!("Shows values robust to occasional outliers"));
                                    ui.checkbox(
                                        &mut h.display.show_rtp_jitter,
                                        tr!("RTP Jitter (Current variation per RFC 3550)"),
                                    ).on_hover_text(tr!("Variation in packet delay (jitter)"));
                                    ui.checkbox(
                                        &mut h.display.show_rtp_mean_jitter,
                                        tr!("RTP Jitter Mean (Average variation)"),
                                    ).on_hover_text(tr!("Show average jitter over history"));
                                    ui.checkbox(
                                        &mut h.display.show_rtp_median_jitter,
                                        tr!("RTP Jitter Median (Middle variation value)"),
                                    ).on_hover_text(tr!("Show median jitter over history"));
                                    ui.checkbox(
                                        &mut h.display.show_mos,
                                        tr!("MOS (Estimated Voice Quality, 1.0-4.5)"),
                                    ).on_hover_text(tr!("Calculated Voice Quality (Mean Opinion Score)"));
                                    ui.checkbox(
                                        &mut h.display.show_availability,
                                        tr!("Availability (Packet delivery success rate %)"),
                                    ).on_hover_text(tr!("Percentage of packets that received a reply"));
                                    ui.checkbox(
                                        &mut h.display.show_outliers,
                                        tr!("Outliers (Packets significantly slower than average)"),
                                    ).on_hover_text(tr!("Number of packets with latency > mean + 3*stddev"));
                                    ui.checkbox(
                                        &mut h.display.show_streak,
                                        tr!("Streak (Consecutive success/fail count)"),
                                    ).on_hover_text(tr!("Shows current series of successes or failures"));
                                    ui.checkbox(
                                        &mut h.display.show_stddev,
                                        tr!("Standard Deviation (RTT stability measure)"),
                                    ).on_hover_text(tr!("Measure of how stable the latency is"));
                                    ui.checkbox(
                                        &mut h.display.show_p95,
                                        tr!("95th Percentile (Latency for 95% of packets)"),
                                    ).on_hover_text(tr!("Worst-case latency for 95% of traffic"));
                                    ui.checkbox(
                                        &mut h.display.show_min_max,
                                        tr!("Min / Max (Extreme latency values)"),
                                    ).on_hover_text(tr!("Shows absolute minimum and maximum in history"));
                                    ui.checkbox(
                                        &mut h.display.show_loss,
                                        tr!("Loss Statistics (Sent/Lost counters)"),
                                    ).on_hover_text(tr!("Shows raw packet counters (sent and lost)"));

                                    ui.add_space(12.0);
                                    ui.button(tr!("Close")).clicked()
                                });

                            if let Some(inner_res) = window_res {
                                if inner_res.inner == Some(true) {
                                    is_open = false;
                                }
                            }

                            // Зберігаємо зміни
                            let mut state = self.state.lock().unwrap();
                            if let Some(target) =
                                state.hosts.iter_mut().find(|th| th.address == *addr)
                            {
                                *target = h;
                            }
                        }

                        // У разі якщо кнопка "Close" була натиснута (ми можемо перевірити повернення .show, але простіше так)
                        // Або якщо було натиснуто 'x' у заголовку вікна (це оновить is_open)
                        if !is_open {
                            self.editing_host = None;
                        }
                    }

                    // Модальне вікно довідки
                    if self.help_window_open {
                        let mut open_var = true;
                        let window_res = egui::Window::new(tr!("Network Statistics Information"))
                            .open(&mut open_var)
                            .resizable(true)
                            .default_width(450.0)
                            .show(ctx, |ui| {
                                ui.horizontal(|ui| {
                                    ui.selectable_value(&mut self.selected_help_tab, HelpTab::Latency, tr!("Latency"));
                                    ui.selectable_value(&mut self.selected_help_tab, HelpTab::Jitter, tr!("Jitter"));
                                    ui.selectable_value(&mut self.selected_help_tab, HelpTab::Quality, tr!("Quality & MOS"));
                                    ui.selectable_value(&mut self.selected_help_tab, HelpTab::Reliability, tr!("Reliability"));
                                });
                                ui.separator();
                                egui::ScrollArea::vertical().show(ui, |ui| {
                                    match self.selected_help_tab {
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
                                        }
                                    }
                                });
                                ui.add_space(8.0);
                                ui.button(tr!("Close")).clicked()
                            });
                        if !open_var || (window_res.is_some() && window_res.unwrap().inner == Some(true)) {
                            self.help_window_open = false;
                        }
                    }
                })
            })
        });
    }
}

impl eframe::App for EguiPinger {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        let serialized = serde_json::to_string_pretty(&self.state).unwrap_or_default();
        storage.set_string(eframe::APP_KEY, serialized);
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.ui_layout(ctx);
        ctx.request_repaint_after(Duration::from_millis(1000));
    }
}
fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(tr!("egui pinger"))
            .with_inner_size([800.0, 520.0])
            .with_resizable(true),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };

    #[cfg(not(feature = "embed-locales"))]
    tr_init!("./locales");

    #[cfg(feature = "embed-locales")]
    {
        // For embedded mode, we check the language and load the appropriate MO file.
        // Currently, we only have Ukrainian translation.
        let lang = std::env::var("LANG")
            .or_else(|_| std::env::var("LC_ALL"))
            .or_else(|_| std::env::var("LC_MESSAGES"))
            .unwrap_or_else(|_| "en".to_string());

        if lang.starts_with("uk") {
            let uk_mo = include_bytes!("../locales/uk/LC_MESSAGES/egui_pinger.mo");
            if let Ok(translator) = MoTranslator::from_vec_u8(uk_mo.to_vec()) {
                tr::set_translator!(translator);
            }
        }
    }

    eframe::run_native(
        "egui_pinger",
        options,
        Box::new(|cc| Ok(Box::new(EguiPinger::new(cc)))),
    )
}

#[cfg(test)]
mod gui_tests {
    use super::*;
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;

    #[test]
    fn test_add_host_flow() {
        let state = Arc::new(Mutex::new(AppState::default()));
        let mut app = EguiPinger::from_state(state.clone());
        // 1. Set fields (before creating harness to avoid borrow checker issues)
        app.input_name = "Google".to_string();
        app.input_address = "8.8.8.8".to_string();

        let mut harness = Harness::new(|ctx| app.ui_layout(ctx));

        // 2. Click Add
        harness.get_by_label("Add").click();
        harness.run();

        // 4. Verify
        let state_lock = state.lock().unwrap();
        assert_eq!(state_lock.hosts.len(), 1);
        assert_eq!(state_lock.hosts[0].name, "Google");
        assert_eq!(state_lock.hosts[0].address, "8.8.8.8");
    }

    #[test]
    fn test_remove_host_flow() {
        let state = Arc::new(Mutex::new(AppState::default()));
        {
            let mut s = state.lock().unwrap();
            s.hosts.push(HostInfo {
                name: "Test".to_string(),
                address: "1.2.3.4".to_string(),
                mode: PingMode::Fast,
                display: DisplaySettings::default(),
            });
            s.statuses
                .insert("1.2.3.4".to_string(), HostStatus::default());
        }

        let mut app = EguiPinger::from_state(state.clone());
        let mut harness = Harness::new(|ctx| app.ui_layout(ctx));
        // Збільшуємо розмір, щоб кнопка видалення (справа) була видима і доступна для кліку
        harness.set_size(egui::vec2(1200.0, 800.0));
        harness.run();

        // Check if host is there
        assert_eq!(state.lock().unwrap().hosts.len(), 1);

        // Click delete button (labeled "x")
        harness.get_by_label("x").click();
        harness.run();

        // Click "Delete" in the confirmation dialog
        harness.get_by_label(&tr!("Delete")).click();
        harness.run();

        // Verify host is gone
        assert!(state.lock().unwrap().hosts.is_empty());
    }

    #[test]
    fn test_validation_empty_address() {
        let state = Arc::new(Mutex::new(AppState::default()));
        let mut app = EguiPinger::from_state(state.clone());
        // Fill name, address is empty
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

        // The text should contain "123ms"
        // get_by_label for colored_label uses the text as label
        harness.get_by_label_contains("123ms");
    }
}
