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
    #[serde(skip, default)]
    alive: bool,
    #[serde(skip, default)]
    latency: Duration,
    #[serde(skip, default)]
    history: Vec<f64>, // останні 99 RTT у мілісекундах (NaN = втрата)
    #[serde(skip, default)]
    jitter_3: f64,
    #[serde(skip, default)]
    jitter_21: f64,
    #[serde(skip, default)]
    jitter_99: f64,
}

#[derive(Default, serde::Serialize, serde::Deserialize)]
struct AppState {
    hosts: Vec<HostInfo>,
    statuses: HashMap<String, HostStatus>,
}

struct EguiPinger {
    state: SharedState,

    rx: mpsc::UnboundedReceiver<(String, bool, Duration, f64)>,
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

async fn pinger_task(state: SharedState, tx: mpsc::UnboundedSender<(String, bool, Duration, f64)>) {
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
                    let result = tokio::time::timeout(Duration::from_secs(2), ping(ip, &payload)).await;
                    let (alive, rtt) = match result {
                        Ok(Ok((_, duration))) => (true, duration),
                        _ => (false, Duration::ZERO),
                    };
                    let rtt_ms = if alive {
                        rtt.as_secs_f64() * 1000.0
                    } else {
                        f64::NAN
                    };
                    let _res = tx.send((address, alive, rtt, rtt_ms));
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
        while let Ok((address, alive, latency, rtt_ms)) = self.rx.try_recv() {
            let mut state = self.state.lock().unwrap();
            if let Some(status) = state.statuses.get_mut(&address) {
                status.alive = alive;
                status.latency = latency;

                // Додаємо в історію (максимум 99)
                status.history.push(rtt_ms);
                if status.history.len() > 99 {
                    status.history.remove(0);
                }

                // Функція для jitter (стандартне відхилення)
                fn jitter(samples: &[f64]) -> f64 {
                    let valid: Vec<f64> = samples.iter().copied().filter(|v| !v.is_nan()).collect();
                    if valid.len() < 2 {
                        return 0.0;
                    }
                    let mean = valid.iter().sum::<f64>() / valid.len() as f64;
                    let variance =
                        valid.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / valid.len() as f64;
                    variance.sqrt()
                }

                if status.history.len() >= 3 {
                    let last3 = &status.history[status.history.len().saturating_sub(3)..];
                    status.jitter_3 = jitter(last3);
                }
                if status.history.len() >= 21 {
                    let last21 = &status.history[status.history.len().saturating_sub(21)..];
                    status.jitter_21 = jitter(last21);
                }
                if status.history.len() >= 99 {
                    status.jitter_99 = jitter(&status.history);
                }
            }
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Назва:");
                ui.text_edit_singleline(&mut self.input_name);
                ui.label("Адреса:");
                ui.text_edit_singleline(&mut self.input_address);
                if ui.button("Додати").clicked() && !self.input_address.trim().is_empty() {
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
                    if status.latency.as_millis() > 100 {
                        Color32::from_rgb(255, 255, 100)
                    } else {
                        Color32::from_rgb(0, 255, 100)
                    }
                } else {
                    Color32::from_rgb(255, 80, 80)
                };

                let jitter_text = format!(
                    "J3:{:4.1} J21:{:4.1} J99:{:4.1}",
                    status.jitter_3, status.jitter_21, status.jitter_99
                );
                let text = if status.alive {
                    format!(
                        "{:<20} {:<15} → {:4}мс {}",
                        host_info.name,
                        host_info.address,
                        status.latency.as_millis(),
                        jitter_text
                    )
                } else {
                    format!(
                        "{:<20} {:<15} → ВПАВ {}",
                        host_info.name, host_info.address, jitter_text
                    )
                };

                ui.horizontal(|ui| {
                    ui.colored_label(color, RichText::new(text).monospace().strong());

                    // Графік — тоненькі стовпчики
                    let chart = BarChart::new(
                        status
                            .history
                            .iter()
                            .enumerate()
                            .map(|(i, &rtt)| {
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

                    Plot::new(format!("plot_{}", &host_info.address))
                        .height(30.0)
                        .width(300.0)
                        .show_axes(false)
                        .show_grid(false)
                        .allow_zoom(true)
                        .allow_drag(false)
                        .allow_scroll(false)
                        .include_y(0.0)
                        .include_y(100.0)
                        .show(ui, |plot_ui| plot_ui.bar_chart(chart));

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("x").clicked() {
                            to_remove.push(host_info.address.clone());
                        }
                    });
                });
            }

            // Видаляємо хости поза lock
            if !to_remove.is_empty() {
                let mut state = self.state.lock().unwrap();
                for address in to_remove {
                    state.hosts.retain(|x| x.address != address);
                    state.statuses.remove(&address);
                }
            }
        });

        ctx.request_repaint_after(Duration::from_millis(500));
    }

}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([800.0, 520.0])
            .with_resizable(true),
        ..Default::default()
    };

    eframe::run_native(
        "egui pinger",
        options,
        Box::new(|cc| Ok(Box::new(EguiPinger::new(cc)))),
    )
}
