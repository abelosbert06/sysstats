use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const PLIST_LABEL: &str = "com.sysstats.agent";

fn get_plist_path() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or_else(|| "Could not determine user home directory".to_string())?;
    let launch_agents = home.join("Library").join("LaunchAgents");
    let _ = fs::create_dir_all(&launch_agents);
    Ok(launch_agents.join(format!("{}.plist", PLIST_LABEL)))
}

fn get_log_dir() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or_else(|| "Could not determine user home directory".to_string())?;
    let logs = home.join("Library").join("Logs");
    let _ = fs::create_dir_all(&logs);
    Ok(logs)
}

pub fn install(binary_path: &Path) -> Result<(), String> {
    let plist_path = get_plist_path()?;
    let log_dir = get_log_dir()?;
    let out_log = log_dir.join("sys_stats.log");
    let err_log = log_dir.join("sys_stats.err");

    let binary_str = binary_path
        .to_str()
        .ok_or_else(|| "Invalid binary path".to_string())?;

    let plist_content = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{label}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{binary}</string>
        <string>run</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>StandardOutPath</key>
    <string>{out_log}</string>
    <key>StandardErrorPath</key>
    <string>{err_log}</string>
</dict>
</plist>
"#,
        label = PLIST_LABEL,
        binary = binary_str,
        out_log = out_log.display(),
        err_log = err_log.display()
    );

    fs::write(&plist_path, plist_content).map_err(|e| e.to_string())?;

    // Unload previous if already loaded
    let _ = Command::new("launchctl")
        .args(["unload", plist_path.to_str().unwrap()])
        .output();

    // Load and enable new service
    let output = Command::new("launchctl")
        .args(["load", "-w", plist_path.to_str().unwrap()])
        .output()
        .map_err(|e| format!("Failed to execute launchctl: {}", e))?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(format!("launchctl load failed: {}", err));
    }

    println!("launchd service successfully registered at {}", plist_path.display());
    Ok(())
}

pub fn start() -> Result<(), String> {
    let plist_path = get_plist_path()?;
    let output = Command::new("launchctl")
        .args(["load", "-w", plist_path.to_str().unwrap()])
        .output()
        .map_err(|e| e.to_string())?;

    if !output.status.success() {
        let _ = Command::new("launchctl")
            .args(["start", PLIST_LABEL])
            .output();
    }
    println!("Service started.");
    Ok(())
}

pub fn stop() -> Result<(), String> {
    let plist_path = get_plist_path()?;
    let output = Command::new("launchctl")
        .args(["unload", plist_path.to_str().unwrap()])
        .output()
        .map_err(|e| e.to_string())?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Failed to stop service: {}", err));
    }
    println!("Service stopped.");
    Ok(())
}

pub fn status() -> Result<String, String> {
    let output = Command::new("launchctl")
        .args(["list", PLIST_LABEL])
        .output()
        .map_err(|e| e.to_string())?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(format!("Active (launchd)\n{}", stdout))
    } else {
        Ok("Inactive (not running or not loaded)".to_string())
    }
}

pub fn uninstall() -> Result<(), String> {
    let plist_path = get_plist_path()?;
    if plist_path.exists() {
        let _ = Command::new("launchctl")
            .args(["unload", "-w", plist_path.to_str().unwrap()])
            .output();

        fs::remove_file(&plist_path).map_err(|e| e.to_string())?;
        println!("Service removed: {}", plist_path.display());
    } else {
        println!("Service is not installed.");
    }
    Ok(())
}
