fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default() == "windows" {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/linux/com.github.vlisivka.EguiPinger.ico");
        if let Err(e) = res.compile() {
            eprintln!("Error: failed to compile Windows resources: {}", e);
            std::process::exit(1);
        }
    }
}
