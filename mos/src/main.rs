use std::io::{BufRead};
use mos::{DisplaySettings};

fn main() {
    // Collect and parse command line arguments using manual matching to avoid dependencies.
    let args: Vec<String> = std::env::args().collect();
    let mut args_slice = &args[1..];

    let mut window_size = 300;
    let mut display = DisplaySettings::default();

    // Loop through arguments using list-pattern matching (tail @ ..)
    while let [head, tail @ ..] = args_slice {
        match head.as_str() {
            "--help" | "-h" => {
                println!("mos: CLI utility for ping statistics calculation");
                println!("Usage: ping <host> | mos [OPTIONS]");
                println!("\nOptions:");
                println!(
                    "  -n, --number-of-lines NUM  Number of lines for statistics (default: 300)"
                );
                println!(
                    "  -e, --enable STAT          Enable field: mean|median|p95|jitter|jitter_mean|jitter_median|mos|loss|availability|outliers|stddev|minmax|streak"
                );
                println!(
                    "  -d, --disable STAT         Disable field: mean|median|p95|jitter|jitter_mean|jitter_median|mos|loss|availability|outliers|stddev|minmax|streak"
                );
                println!("  -h, --help                 Show this help message");
                return;
            }
            "-n" | "--number-of-lines" => {
                if let [num_str, rest @ ..] = tail {
                    if let Ok(num) = num_str.parse::<usize>() {
                        window_size = num;
                    }
                    args_slice = rest;
                } else {
                    eprintln!("Error: -n|--number-of-lines requires a numeric value");
                    std::process::exit(1);
                }
            }
            "-e" | "--enable" => {
                if let [stat, rest @ ..] = tail {
                    mos::toggle_stat(&mut display, stat, true);
                    args_slice = rest;
                } else {
                    eprintln!("Error: -e|--enable requires a statistic name");
                    std::process::exit(1);
                }
            }
            "-d" | "--disable" => {
                if let [stat, rest @ ..] = tail {
                    mos::toggle_stat(&mut display, stat, false);
                    args_slice = rest;
                } else {
                    eprintln!("Error: -d|--disable requires a statistic name");
                    std::process::exit(1);
                }
            }
            _ => {
                eprintln!("Error: Unknown argument: {}", head);
                std::process::exit(1);
            }
        }
    }

    let stdin = std::io::stdin();
    let stdout = std::io::stdout();

    if let Err(e) = mos::run_loop(stdin.lock(), stdout.lock(), window_size, &display) {
        if e.kind() != std::io::ErrorKind::BrokenPipe {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    }
}
