#[cfg(unix)]
mod unix_impl {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::process::Command;

    const SERVICE_NAME: &str = "sys-stats";

    fn is_root() -> bool {
        let output = Command::new("id").arg("-u").output();
        if let Ok(out) = output {
            String::from_utf8_lossy(&out.stdout).trim() == "0"
        } else {
            false
        }
    }

    fn get_unit_path() -> Result<PathBuf, String> {
        if is_root() {
            Ok(PathBuf::from(format!("/etc/systemd/system/{}.service", SERVICE_NAME)))
        } else {
            let home = dirs::home_dir().ok_or_else(|| "Could not find user home directory".to_string())?;
            let user_systemd = home.join(".config").join("systemd").join("user");
            let _ = fs::create_dir_all(&user_systemd);
            Ok(user_systemd.join(format!("{}.service", SERVICE_NAME)))
        }
    }

    fn run_systemctl(args: &[&str]) -> Result<std::process::Output, String> {
        let mut cmd = Command::new("systemctl");
        if !is_root() {
            cmd.arg("--user");
        }
        cmd.args(args);
        cmd.output().map_err(|e| format!("Failed to execute systemctl: {}", e))
    }

    pub fn install(binary_path: &Path) -> Result<(), String> {
        let unit_path = get_unit_path()?;
        let binary_str = binary_path
            .to_str()
            .ok_or_else(|| "Invalid binary path".to_string())?;

        let wanted_by = if is_root() { "multi-user.target" } else { "default.target" };

        let unit_content = format!(
            r#"[Unit]
Description=SysStats Telemetry Agent
After=network.target

[Service]
Type=simple
ExecStart={binary} run
Restart=always
RestartSec=5

[Install]
WantedBy={wanted_by}
"#,
            binary = binary_str,
            wanted_by = wanted_by
        );

        fs::write(&unit_path, unit_content).map_err(|e| e.to_string())?;

        run_systemctl(&["daemon-reload"])?;
        run_systemctl(&["enable", "--now", SERVICE_NAME])?;

        println!("systemd unit registered at {}", unit_path.display());
        println!("Service enabled and started.");
        Ok(())
    }

    pub fn start() -> Result<(), String> {
        let output = run_systemctl(&["start", SERVICE_NAME])?;
        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr);
            return Err(format!("systemctl start failed: {}", err));
        }
        println!("Service started.");
        Ok(())
    }

    pub fn stop() -> Result<(), String> {
        let output = run_systemctl(&["stop", SERVICE_NAME])?;
        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr);
            return Err(format!("systemctl stop failed: {}", err));
        }
        println!("Service stopped.");
        Ok(())
    }

    pub fn status() -> Result<String, String> {
        let output = run_systemctl(&["status", SERVICE_NAME])?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        if output.status.success() {
            Ok(format!("Active (systemd)\n{}", stdout))
        } else {
            Ok(format!("Inactive / Error\n{}{}", stdout, stderr))
        }
    }

    pub fn uninstall() -> Result<(), String> {
        let _ = run_systemctl(&["stop", SERVICE_NAME]);
        let _ = run_systemctl(&["disable", SERVICE_NAME]);

        let unit_path = get_unit_path()?;
        if unit_path.exists() {
            fs::remove_file(&unit_path).map_err(|e| e.to_string())?;
            let _ = run_systemctl(&["daemon-reload"]);
            println!("Service removed: {}", unit_path.display());
        } else {
            println!("Service is not installed.");
        }
        Ok(())
    }
}

#[cfg(unix)]
pub use unix_impl::*;

#[cfg(not(unix))]
mod non_unix_impl {
    use std::path::Path;

    pub fn install(_: &Path) -> Result<(), String> {
        Err("systemd is not supported on this platform.".to_string())
    }
    pub fn start() -> Result<(), String> {
        Err("systemd is not supported on this platform.".to_string())
    }
    pub fn stop() -> Result<(), String> {
        Err("systemd is not supported on this platform.".to_string())
    }
    pub fn status() -> Result<String, String> {
        Err("systemd is not supported on this platform.".to_string())
    }
    pub fn uninstall() -> Result<(), String> {
        Err("systemd is not supported on this platform.".to_string())
    }
}

#[cfg(not(unix))]
pub use non_unix_impl::*;
