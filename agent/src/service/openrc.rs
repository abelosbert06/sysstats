#[cfg(unix)]
mod unix_impl {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};
    use std::process::Command;

    const INIT_SCRIPT_PATH: &str = "/etc/init.d/sys_stats";
    const SERVICE_NAME: &str = "sys_stats";

    #[allow(dead_code)]
    pub fn is_openrc_available() -> bool {
        Path::new("/sbin/openrc-run").exists() || Path::new("/run/openrc").exists() || Command::new("rc-service").arg("-V").output().is_ok()
    }

    pub fn install(binary_path: &Path) -> Result<(), String> {
        let binary_str = binary_path
            .to_str()
            .ok_or_else(|| "Invalid binary path".to_string())?;

        let script_content = format!(
            r#"#!/sbin/openrc-run
name="sys_stats"
description="SysStats Telemetry Agent"
command="{binary}"
command_args="run"
command_background="yes"
pidfile="/run/sys_stats.pid"

depend() {{
    need net
}}
"#,
            binary = binary_str
        );

        let script_path = PathBuf::from(INIT_SCRIPT_PATH);
        fs::write(&script_path, script_content).map_err(|e| format!("Failed to write OpenRC script (are you root/sudo?): {}", e))?;

        let mut perms = fs::metadata(&script_path).map_err(|e| e.to_string())?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script_path, perms).map_err(|e| e.to_string())?;

        let _ = Command::new("rc-update")
            .args(["add", SERVICE_NAME, "default"])
            .output();

        let output = Command::new("rc-service")
            .args([SERVICE_NAME, "start"])
            .output()
            .map_err(|e| format!("Failed to execute rc-service: {}", e))?;

        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr);
            return Err(format!("rc-service start failed: {}", err));
        }

        println!("OpenRC service registered at {}", INIT_SCRIPT_PATH);
        println!("Service added to default runlevel and started.");
        Ok(())
    }

    pub fn start() -> Result<(), String> {
        let output = Command::new("rc-service")
            .args([SERVICE_NAME, "start"])
            .output()
            .map_err(|e| e.to_string())?;

        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr);
            return Err(format!("rc-service start failed: {}", err));
        }
        println!("Service started.");
        Ok(())
    }

    pub fn stop() -> Result<(), String> {
        let output = Command::new("rc-service")
            .args([SERVICE_NAME, "stop"])
            .output()
            .map_err(|e| e.to_string())?;

        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr);
            return Err(format!("rc-service stop failed: {}", err));
        }
        println!("Service stopped.");
        Ok(())
    }

    pub fn status() -> Result<String, String> {
        let output = Command::new("rc-service")
            .args([SERVICE_NAME, "status"])
            .output()
            .map_err(|e| e.to_string())?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        if output.status.success() {
            Ok(format!("Active (OpenRC)\n{}", stdout))
        } else {
            Ok(format!("Inactive / Error\n{}{}", stdout, stderr))
        }
    }

    pub fn uninstall() -> Result<(), String> {
        let _ = Command::new("rc-service").args([SERVICE_NAME, "stop"]).output();
        let _ = Command::new("rc-update").args(["del", SERVICE_NAME, "default"]).output();

        let path = PathBuf::from(INIT_SCRIPT_PATH);
        if path.exists() {
            fs::remove_file(&path).map_err(|e| e.to_string())?;
            println!("OpenRC service removed: {}", INIT_SCRIPT_PATH);
        } else {
            println!("OpenRC service is not installed.");
        }
        Ok(())
    }
}

#[cfg(unix)]
pub use unix_impl::*;

#[cfg(not(unix))]
mod non_unix_impl {
    use std::path::Path;

    #[allow(dead_code)]
    pub fn is_openrc_available() -> bool {
        false
    }
    pub fn install(_: &Path) -> Result<(), String> {
        Err("OpenRC is not supported on this platform.".to_string())
    }
    pub fn start() -> Result<(), String> {
        Err("OpenRC is not supported on this platform.".to_string())
    }
    pub fn stop() -> Result<(), String> {
        Err("OpenRC is not supported on this platform.".to_string())
    }
    pub fn status() -> Result<String, String> {
        Err("OpenRC is not supported on this platform.".to_string())
    }
    pub fn uninstall() -> Result<(), String> {
        Err("OpenRC is not supported on this platform.".to_string())
    }
}

#[cfg(not(unix))]
pub use non_unix_impl::*;
