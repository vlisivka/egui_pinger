#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release

use eframe::egui;
#[cfg(feature = "embed-locales")]
use tr::MoTranslator;
use tr::tr;
#[cfg(not(feature = "embed-locales"))]
use tr::tr_init;

use egui_pinger::EguiPinger;

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
