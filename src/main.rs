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
use model::{AppState, HostInfo, HostStatus};

pub struct EguiPinger {
    pub(crate) state: SharedState,
    pub(crate) input_name: String,
    pub(crate) input_address: String,
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
        // Встановлюємо початкові налаштування теми
        cc.egui_ctx.options_mut(|o| {
            o.theme_preference = egui::ThemePreference::System;
        });

        // Спробуємо встановити тему згідно з системною за допомогою Theme
        if let Some(theme) = cc.egui_ctx.input(|i| i.raw.system_theme) {
            cc.egui_ctx.set_visuals(theme.default_visuals());
        }

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
        }
    }

    pub fn from_state(state: SharedState) -> Self {
        Self {
            state,
            input_name: String::new(),
            input_address: String::new(),
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
                                state.hosts.push(HostInfo { name, address });
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
                    let mut to_remove = Vec::new();
                    let default_host_status = HostStatus::default();

                    for host_info in &hosts {
                        let status = statuses
                            .get(&host_info.address)
                            .unwrap_or(&default_host_status);

                        let color = visuals.status_color(status.alive, status.latency);

                        let jitter_text = format!(
                            "{} {:4.1} {} Т3 {:4.1}, Т21 {:4.1}, Т99 {:4.1}",
                            tr!("Mean:"),
                            status.mean,
                            tr!("Jitter:"),
                            status.jitter_3,
                            status.jitter_21,
                            status.jitter_99
                        );
                        let text = if status.alive {
                            format!(
                                "{:<20} {:<15} → {:4.0}ms {} {}: {}/{} {:.2}%",
                                host_info.name,
                                host_info.address,
                                status.latency,
                                jitter_text,
                                tr!("Lost"),
                                status.lost,
                                status.sent,
                                (status.lost as f64
                                    / if status.sent == 0 { 1 } else { status.sent } as f64)
                                    * 100.0
                            )
                        } else {
                            format!(
                                "{:<20} {:<15} → {:>4} {} {}: {}/{} {:.2}%",
                                host_info.name,
                                host_info.address,
                                tr!("DOWN"),
                                jitter_text,
                                tr!("Lost"),
                                status.lost,
                                status.sent,
                                (status.lost as f64
                                    / if status.sent == 0 { 1 } else { status.sent } as f64)
                                    * 100.0
                            )
                        };

                        ui.horizontal(|ui| {
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
                            // Щоб 99 стовпчиків шириною 1.0 заповнювали весь простір без "чорних смужок":
                            // 1. Встановлюємо межі X від -0.5 до 98.5 (разом 99 одиниць).
                            // 2. Прибираємо горизонтальні відступи (margin_fraction).
                            Plot::new(format!("plot_{}", &host_info.address))
                                .height(30.0)
                                .width(337.0)
                                .show_axes(false)
                                .show_grid(false)
                                .allow_zoom(false)
                                .allow_drag(false)
                                .allow_scroll(false)
                                .set_margin_fraction(egui::Vec2::new(0.0, 0.05))
                                .include_x(-0.5)
                                .include_x(98.5)
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
                            ui.colored_label(color, RichText::new(text).monospace().strong());

                            // Кнопка для видалення хоста (справа)
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui.button("x").clicked() {
                                        to_remove.push(host_info.address.clone());
                                    }
                                },
                            );
                        });
                    }

                    // Видаляємо хости, які були позначені для видалення
                    if !to_remove.is_empty() {
                        let mut state = self.state.lock().unwrap();
                        for address in to_remove {
                            state.hosts.retain(|x| x.address != address);
                            state.statuses.remove(&address);
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
        &tr!("egui pinger"),
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
