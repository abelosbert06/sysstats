use clap::{Args, Parser, Subcommand};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::io::{self, Write};
use sysinfo::System;

mod collector;
mod config;
mod service;

const SUPABASE_URL: &str = "https://mdeubpsdmmuowntstjjv.supabase.co";
const SUPABASE_ANON_KEY: &str = "sb_publishable_teBG-z74PbFwZKkgsOyrUw_gE8nVyWB";

#[derive(Parser, Debug)]
#[command(
    name = "sys_stats",
    about = "SysStats Lightweight Telemetry Agent & Background Service",
    version = "0.1.0"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Legacy flag: run collector directly
    #[arg(long, hide = true)]
    headless: bool,

    /// Legacy flag: token override
    #[arg(long, hide = true)]
    token: Option<String>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Register device with a pre-generated token from the web dashboard
    Register {
        /// Device Token generated from the Web Dashboard
        #[arg(short, long)]
        token: String,
    },

    /// Interactively log in to your account and register this device automatically
    Login,

    /// Manage the background service (systemd, OpenRC, launchd, Windows)
    Service(ServiceArgs),

    /// Run the telemetry collector loop
    Run {
        /// Optional token override
        #[arg(short, long)]
        token: Option<String>,
    },

    /// View local agent configuration and test backend connectivity
    Status,
}

#[derive(Args, Debug)]
struct ServiceArgs {
    #[command(subcommand)]
    action: ServiceAction,

    /// Specific init system manager (default: auto-detect)
    #[arg(long, value_enum, default_value = "auto", global = true)]
    manager: service::InitManager,
}

#[derive(Subcommand, Debug)]
enum ServiceAction {
    /// Install and enable the background service
    Install {
        /// Optional device token to configure during installation
        #[arg(short, long)]
        token: Option<String>,
    },
    /// Start the background service
    Start,
    /// Stop the background service
    Stop,
    /// Check background service status
    Status,
    /// Uninstall and remove the background service
    Uninstall,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Register { token }) => {
            config::set_token(&token).map_err(|e| format!("Failed to save token: {}", e))?;
            println!("Device token saved successfully to {}", config::get_config_path().display());
            println!("To start background monitoring, run: sys_stats service install");
        }

        Some(Commands::Login) => {
            handle_login().await?;
        }

        Some(Commands::Service(args)) => {
            match args.action {
                ServiceAction::Install { token } => {
                    service::install(args.manager, token).map_err(|e| format!("Install error: {}", e))?;
                }
                ServiceAction::Start => {
                    service::start(args.manager).map_err(|e| format!("Start error: {}", e))?;
                }
                ServiceAction::Stop => {
                    service::stop(args.manager).map_err(|e| format!("Stop error: {}", e))?;
                }
                ServiceAction::Status => {
                    let st = service::status(args.manager).map_err(|e| format!("Status error: {}", e))?;
                    println!("{}", st);
                }
                ServiceAction::Uninstall => {
                    service::uninstall(args.manager).map_err(|e| format!("Uninstall error: {}", e))?;
                }
            }
        }

        Some(Commands::Run { token }) => {
            collector::run_collector_loop(token).await.map_err(|e| format!("Collector error: {}", e))?;
        }

        Some(Commands::Status) => {
            handle_status().await?;
        }

        None => {
            // Handle legacy flags if user calls `sys_stats --headless --token ...`
            if cli.headless || cli.token.is_some() {
                collector::run_collector_loop(cli.token).await.map_err(|e| format!("Collector error: {}", e))?;
            } else {
                println!("SysStats Telemetry Agent");
                println!("Run `sys_stats --help` for available commands.");
                let _ = handle_status().await;
            }
        }
    }

    Ok(())
}

async fn handle_login() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== SysStats Account Login ===");
    print!("Email: ");
    io::stdout().flush()?;
    let mut email = String::new();
    io::stdin().read_line(&mut email)?;
    let email = email.trim().to_string();

    let password = rpassword::prompt_password("Password: ")?;

    println!("\nAuthenticating with Supabase...");
    let client = Client::new();
    let token_url = format!("{}/auth/v1/token?grant_type=password", SUPABASE_URL);

    let auth_res = client
        .post(&token_url)
        .header("apikey", SUPABASE_ANON_KEY)
        .json(&json!({ "email": email, "password": password }))
        .send()
        .await?;

    if !auth_res.status().is_success() {
        eprintln!("Login failed: Invalid credentials or unverified account.");
        return Ok(());
    }

    #[derive(Deserialize)]
    struct AuthResponse {
        access_token: String,
    }

    let auth_data: AuthResponse = auth_res.json().await?;
    let jwt = auth_data.access_token;

    println!("Login successful!");

    let hostname = System::host_name().unwrap_or_else(|| "Desktop Machine".to_string());
    print!("Register device name [default: {}]: ", hostname);
    io::stdout().flush()?;
    let mut dev_name = String::new();
    io::stdin().read_line(&mut dev_name)?;
    let dev_name = if dev_name.trim().is_empty() {
        hostname
    } else {
        dev_name.trim().to_string()
    };

    println!("Registering device '{}' on your account...", dev_name);
    let cfg = config::load_config();
    let register_url = format!("{}/api/devices", cfg.api_url.trim_end_matches('/'));

    #[derive(Serialize)]
    struct RegReq {
        name: String,
    }

    #[derive(Deserialize)]
    struct RegResp {
        device_id: String,
        auth_token: String,
    }

    let reg_res = client
        .post(&register_url)
        .bearer_auth(&jwt)
        .json(&RegReq { name: dev_name.clone() })
        .send()
        .await?;

    if !reg_res.status().is_success() {
        eprintln!("Failed to register device with backend API.");
        return Ok(());
    }

    let reg_data: RegResp = reg_res.json().await?;

    let mut new_cfg = cfg;
    new_cfg.device_id = Some(reg_data.device_id);
    new_cfg.device_token = Some(reg_data.auth_token);
    new_cfg.device_name = Some(dev_name);
    config::save_config(&new_cfg).map_err(|e| format!("Failed to save config: {}", e))?;

    println!("\nDevice successfully registered and linked!");
    println!("Config saved to {}", config::get_config_path().display());
    println!("\nTo install and start monitoring as a background service:");
    println!("  sys_stats service install");
    println!("Or to run directly in the terminal:");
    println!("  sys_stats run");

    Ok(())
}

async fn handle_status() -> Result<(), Box<dyn std::error::Error>> {
    let cfg = config::load_config();
    let detected_init = service::detect_manager();

    println!("\n--- SysStats Agent Status ---");
    println!("Config Path:     {}", config::get_config_path().display());
    println!("API Endpoint:    {}", cfg.api_url);
    println!("Device Name:     {}", cfg.device_name.as_deref().unwrap_or("(not registered)"));
    println!("Device Token:    {}", if cfg.device_token.is_some() { "Configured" } else { "Missing" });
    println!("Detected Init:   {:?}", detected_init);

    // Test API connectivity
    print!("API Connectivity: ");
    io::stdout().flush()?;

    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(4))
        .build()?;

    let health_url = format!("{}/api/devices", cfg.api_url.trim_end_matches('/'));
    match client.get(&health_url).send().await {
        Ok(resp) => {
            // 401 is expected if missing token, but proves API is reachable
            if resp.status().is_client_error() || resp.status().is_success() {
                println!("OK (Reachable)");
            } else {
                println!("Degraded (HTTP {})", resp.status());
            }
        }
        Err(e) => {
            println!("Unreachable ({})", e);
        }
    }

    // Check service status
    println!("\n--- Service Status ---");
    match service::status(service::InitManager::Auto) {
        Ok(st) => println!("{}", st),
        Err(e) => println!("Service check failed: {}", e),
    }

    Ok(())
}
