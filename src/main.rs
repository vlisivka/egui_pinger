#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release

use eframe::egui;
use std::sync::Arc;
#[cfg(any(feature = "embed-locales", target_os = "windows"))]
use tr::MoTranslator;
use tr::tr;
#[cfg(not(any(feature = "embed-locales", target_os = "windows")))]
use tr::tr_init;

use egui_pinger::EguiPinger;

fn main() -> eframe::Result {
    let mut viewport = egui::ViewportBuilder::default()
        .with_title(tr!("egui_pinger"))
        .with_inner_size([800.0, 520.0])
        .with_resizable(true);
    let icon_data = eframe::icon_data::from_png_bytes(include_bytes!(
        "../assets/linux/com.github.vlisivka.EguiPinger.png"
    ))
    .expect("The icon data must be valid");
    viewport.icon = Some(Arc::new(icon_data));
    viewport.app_id = Some("com.github.vlisivka.EguiPinger".to_string());

    let options = eframe::NativeOptions {
        viewport,
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };

    #[cfg(not(any(feature = "embed-locales", target_os = "windows")))]
    tr_init!("./locales"); // TODO: use the system locale directory when built in release mode

    #[cfg(any(feature = "embed-locales", target_os = "windows"))]
    {
        // For embedded mode, we check the language and load the appropriate MO file.
        // Currently, we only have Ukrainian translation.
        let lang = sys_locale::get_locale()
            .or_else(|| std::env::var("LANG").ok())
            .or_else(|| std::env::var("LC_ALL").ok())
            .or_else(|| std::env::var("LC_MESSAGES").ok())
            .unwrap_or_else(|| "en".to_string());

        if lang.starts_with("uk") {
            let uk_mo = include_bytes!("../locales/uk/LC_MESSAGES/egui_pinger.mo");
            if let Ok(translator) = MoTranslator::from_vec_u8(uk_mo.to_vec()) {
                tr::set_translator!(translator);
            }
        }
    }

    eframe::run_native(
        "com.github.vlisivka.EguiPinger",
        options,
        Box::new(|cc| Ok(Box::new(EguiPinger::new(cc)))),
    )
}
