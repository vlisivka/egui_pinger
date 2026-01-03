use eframe::egui;
use eframe::egui::{Color32, RichText};
use egui_plot::{Bar, BarChart, Plot};
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use surge_ping::ping;
use tokio::sync::mpsc;

type SharedState = Arc<Mutex<AppState>>;

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
struct HostInfo {
    name: String,
    address: String,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
struct HostStatus {
    /// Чи отримали ми відповідь від хоста цього разу
    #[serde(skip, default)]
    alive: bool,
    /// Останній RTT
    #[serde(skip, default)]
    latency: f64,
    /// Останні 99 RTT у мілісекундах (NaN = втрата)
    #[serde(skip, default)]
    history: Vec<f64>,
    /// Середнє арифметичне
    #[serde(skip, default)]
    mean: f64,
    /// Тремтіння (jitter) для останніх 3 результатів
    #[serde(skip, default)]
    jitter_3: f64,
    /// Тремтіння (jitter) для останніх 21 результатів
    #[serde(skip, default)]
    jitter_21: f64,
    /// Середнє тремтіння
    #[serde(skip, default)]
    jitter_99: f64,
    /// Кількість надісланих пакетів
    #[serde(skip, default)]
    sent: u32,
    /// Кількість неотриманих відповідей
    #[serde(skip, default)]
    lost: u32,
}

impl HostStatus {
    fn add_sample(&mut self, rtt_ms: f64) {
        self.sent += 1;

        if rtt_ms.is_nan() {
            self.lost += 1;
        }

        self.latency = rtt_ms;

        // Додаємо в історію (максимум 99)
        self.history.push(rtt_ms);
        if self.history.len() > 99 {
            self.history.remove(0);
        }

        let valid_data = self
            .history
            .iter()
            .copied()
            .filter(|v| !v.is_nan())
            .collect::<Vec<f64>>();

        if valid_data.len() < 2 {
            // Недостатньо даних
            self.jitter_3 = 0.0;
            self.jitter_99 = 0.0;
            return;
        }

        // Середнє
        self.mean = valid_data.iter().sum::<f64>() / valid_data.len() as f64;

        // Середнє тремтіння (jitter)
        self.jitter_99 = Self::calculate_jitter(&valid_data[..]);

        // Тремтіння для останніх 16 елементів
        let start_index = valid_data.len().saturating_sub(16);
        self.jitter_21 = Self::calculate_jitter(&valid_data[start_index..]);

        // Тремтіння для останніх 3 елементів
        let start_index = valid_data.len().saturating_sub(3);
        self.jitter_3 = Self::calculate_jitter(&valid_data[start_index..]);
    }

    fn calculate_jitter(valid_data: &[f64]) -> f64 {
        if valid_data.len() < 2 {
            return 0.0;
        }

        let mut total_diff = 0.0;
        // Обчислюємо абсолютну різницю між сусідніми елементами
        for window in valid_data.windows(2) {
            let diff = (window[1] - window[0]).abs();
            total_diff += diff;
        }
        // Середнє значення різниць
        total_diff / (valid_data.len() - 1) as f64
    }
}

#[derive(Default, serde::Serialize, serde::Deserialize)]
struct AppState {
    hosts: Vec<HostInfo>,
    statuses: HashMap<String, HostStatus>,
}

struct EguiPinger {
    state: SharedState,

    rx: mpsc::UnboundedReceiver<(String, bool, f64)>,
    input_name: String,
    input_address: String,
}

impl EguiPinger {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();

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
                .block_on(pinger_task(state_clone, tx));
        });

        Self {
            state,
            rx,
            input_name: String::new(),
            input_address: String::new(),
        }
    }
}

use futures::future::join_all;

async fn pinger_task(state: SharedState, tx: mpsc::UnboundedSender<(String, bool, f64)>) {
    let payload = [42u8; 16];
    let mut interval = tokio::time::interval(Duration::from_secs(2));

    loop {
        interval.tick().await;

        let hosts: Vec<HostInfo> = state.lock().unwrap().hosts.clone();
        if hosts.is_empty() {
            continue;
        }

        // Створюємо та запускаємо всі пінги паралельно
        let ping_tasks: Vec<_> = hosts
            .iter()
            .filter_map(|host_info| {
                let ip: IpAddr = host_info.address.parse().ok()?;
                let payload = payload;
                let address = host_info.address.clone();
                let tx = tx.clone();

                Some(tokio::spawn(async move {
                    let result =
                        tokio::time::timeout(Duration::from_secs(2), ping(ip, &payload)).await;

                    let (alive, rtt_ms) = match result {
                        // Є відповідь, хост живий
                        Ok(Ok((_, duration))) => (true, duration.as_secs_f64() * 1000.0),
                        // Немає відповіді, хост впав
                        _ => (false, f64::NAN),
                    };

                    let _res = tx.send((address, alive, rtt_ms));
                }))
            })
            .collect();

        // Запускаємо всі task'и паралельно
        tokio::spawn(async move {
            let _res = join_all(ping_tasks).await;
        });
    }
}

impl eframe::App for EguiPinger {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        let serialized = serde_json::to_string_pretty(&self.state).unwrap_or_default();
        storage.set_string(eframe::APP_KEY, serialized);
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Оновлюємо статуси з фонового потоку
        while let Ok((address, alive, rtt_ms)) = self.rx.try_recv() {
            let mut state = self.state.lock().unwrap();
            if let Some(status) = state.statuses.get_mut(&address) {
                status.alive = alive;

                status.add_sample(rtt_ms);
            }
        }

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
                                .hint_text("Назва хоста")
                                .desired_width(8.0 * 20.0),
                        );

                        let rs2 = ui.add(
                            egui::TextEdit::singleline(&mut self.input_address)
                                .id(addr_field_id)
                                .char_limit(20)
                                .hint_text("Адреса хоста")
                                .desired_width(8.0 * 20.0),
                        );

                        // При натисненні кнопки Додати чи при натисненні клавіші Enter у другому полі,
                        // додати хост до списку
                        if (ui.button("Додати").clicked()
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
                    });

                    ui.separator();

                    // Клонуємо потрібні дані один раз — уникаємо тимчасових значень
                    let (hosts, statuses) = {
                        let state = self.state.lock().unwrap();
                        (state.hosts.clone(), state.statuses.clone())
                    };

                    let mut to_remove = Vec::new();
                    let default_host_status = HostStatus::default();

                    for host_info in &hosts {
                        let status = statuses
                            .get(&host_info.address)
                            .unwrap_or(&default_host_status);

                        let color = if status.alive {
                            if status.latency > 100.0 {
                                Color32::from_rgb(255, 255, 100)
                            } else {
                                Color32::from_rgb(0, 255, 100)
                            }
                        } else {
                            Color32::from_rgb(255, 80, 80)
                        };

                        let jitter_text = format!(
                            "Середнє: {:4.1} Тремтіння: Т3 {:4.1}, Т21 {:4.1}, Т99 {:4.1}",
                            status.mean, status.jitter_3, status.jitter_21, status.jitter_99
                        );
                        let text = if status.alive {
                            format!(
                                "{:<20} {:<15} → {:4.0}мс {} Втрачено: {}/{} {:.2}%",
                                host_info.name,
                                host_info.address,
                                status.latency,
                                jitter_text,
                                status.lost,
                                status.sent,
                                (status.lost as f64
                                    / if status.sent == 0 { 1 } else { status.sent } as f64)
                                    * 100.0
                            )
                        } else {
                            format!(
                                "{:<20} {:<15} →   НЕМА {} Втрачено: {}/{} {:.2}%",
                                host_info.name,
                                host_info.address,
                                jitter_text,
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
                                status
                                    .history
                                    .iter()
                                    .enumerate()
                                    .map(|(i, &rtt)| {
                                        // Якщо пропущений, робимо стовпчик на всю висоту (100 мс)
                                        let height = if rtt.is_nan() { 100.0 } else { rtt };

                                        let fill = if rtt.is_nan() {
                                            Color32::RED
                                        } else if rtt > 100.0 {
                                            Color32::YELLOW
                                        } else {
                                            Color32::from_rgb(0, 200, 100)
                                        };

                                        Bar::new(i as f64, height).width(0.8).fill(fill)
                                    })
                                    .collect(),
                            );

                            // Графік від 0 до 100 мс, по замовчуванню.
                            Plot::new(format!("plot_{}", &host_info.address))
                                .height(30.0)
                                .width(337.0)
                                .show_axes(false)
                                .show_grid(false)
                                .allow_zoom(false)
                                .allow_drag(false)
                                .allow_scroll(false)
                                .include_y(0.0)
                                .include_y(100.0)
                                .show(ui, |plot_ui| plot_ui.bar_chart(chart));

                            // Текст з назвою, адресою, і результатами. Шрифт фіксованої ширини, жирний.
                            ui.colored_label(color, RichText::new(text).monospace().strong());

                            // Кнопка для видалення хоста
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

    eframe::run_native(
        "egui pinger",
        options,
        Box::new(|cc| Ok(Box::new(EguiPinger::new(cc)))),
    )
}
