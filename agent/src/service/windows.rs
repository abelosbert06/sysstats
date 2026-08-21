use std::path::Path;
use std::process::Command;

const TASK_NAME: &str = "SysStatsAgent";

pub fn install(binary_path: &Path) -> Result<(), String> {
    let binary_str = binary_path
        .to_str()
        .ok_or_else(|| "Invalid binary path".to_string())?;

    // Create a Scheduled Task that runs silently in the background on user logon
    let command = format!("\"{}\" run", binary_str);
    let output = Command::new("schtasks")
        .args([
            "/Create",
            "/SC",
            "ONLOGON",
            "/TN",
            TASK_NAME,
            "/TR",
            &command,
            "/F",
        ])
        .output()
        .map_err(|e| format!("Failed to execute schtasks: {}", e))?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Failed to register scheduled task: {}", err));
    }

    // Start immediately
    let _ = Command::new("schtasks")
        .args(["/Run", "/TN", TASK_NAME])
        .output();

    println!("Windows startup task successfully registered.");
    Ok(())
}

pub fn start() -> Result<(), String> {
    let output = Command::new("schtasks")
        .args(["/Run", "/TN", TASK_NAME])
        .output()
        .map_err(|e| e.to_string())?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Failed to run task: {}", err));
    }
    println!("Service started.");
    Ok(())
}

pub fn stop() -> Result<(), String> {
    let output = Command::new("schtasks")
        .args(["/End", "/TN", TASK_NAME])
        .output()
        .map_err(|e| e.to_string())?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Failed to stop task: {}", err));
    }
    println!("Service stopped.");
    Ok(())
}

pub fn status() -> Result<String, String> {
    let output = Command::new("schtasks")
        .args(["/Query", "/TN", TASK_NAME, "/FO", "LIST", "/V"])
        .output()
        .map_err(|e| e.to_string())?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    if output.status.success() {
        Ok(format!("Active (Windows Task)\n{}", stdout))
    } else {
        Ok("Inactive (task not found or disabled)".to_string())
    }
}

pub fn uninstall() -> Result<(), String> {
    let _ = Command::new("schtasks").args(["/End", "/TN", TASK_NAME]).output();
    let output = Command::new("schtasks")
        .args(["/Delete", "/TN", TASK_NAME, "/F"])
        .output()
        .map_err(|e| e.to_string())?;

    if output.status.success() {
        println!("Windows task removed.");
    } else {
        println!("Task was not found.");
    }
    Ok(())
}
