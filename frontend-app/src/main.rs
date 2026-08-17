use clap::Parser;

mod collector;
mod local_db;
mod ui;

#[derive(Parser, Debug)]
#[command(name = "sys_stats", about = "Activity Monitor and Collector")]
pub struct Cli {
    #[arg(long, help = "Run silently in the background as a telemetry collector")]
    pub headless: bool,

    #[arg(long, help = "Device Token for headless mode")]
    pub token: Option<String>,
}

fn main() {
    let cli = Cli::parse();

    if cli.headless {
        #[cfg(not(target_arch = "wasm32"))]
        {
            if let Some(t) = cli.token {
                collector::desktop::set_device_token(t);
            } else if let Some(saved) = local_db::desktop::load_device_token() {
                collector::desktop::set_device_token(saved);
            }
            collector::desktop::start_collector();
            println!("Agent running in headless mode. Press Ctrl+C to stop.");
            std::thread::park();
        }
        #[cfg(target_arch = "wasm32")]
        {
            println!("Headless mode not supported in WebAssembly.");
        }
    } else {
        #[cfg(not(target_arch = "wasm32"))]
        {
            // Load any previously saved device token so the collector syncs immediately
            if let Some(saved) = local_db::desktop::load_device_token() {
                collector::desktop::set_device_token(saved);
            }
            collector::desktop::start_collector();
        }

        dioxus::launch(ui::App);
    }
}
