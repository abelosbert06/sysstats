pub mod launchd;
pub mod openrc;
pub mod systemd;
pub mod windows;

use clap::ValueEnum;
use std::env;
use std::path::PathBuf;

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum InitManager {
    Auto,
    Launchd,
    Systemd,
    Openrc,
    Windows,
}

pub fn detect_manager() -> InitManager {
    #[cfg(target_os = "macos")]
    {
        InitManager::Launchd
    }
    #[cfg(target_os = "windows")]
    {
        InitManager::Windows
    }
    #[cfg(target_os = "linux")]
    {
        if openrc::is_openrc_available() {
            InitManager::Openrc
        } else {
            InitManager::Systemd
        }
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        InitManager::Systemd
    }
}

pub fn resolve_manager(manager: InitManager) -> InitManager {
    if manager == InitManager::Auto {
        detect_manager()
    } else {
        manager
    }
}

fn get_current_exe_path() -> Result<PathBuf, String> {
    env::current_exe().map_err(|e| format!("Failed to get current executable path: {}", e))
}

pub fn install(manager: InitManager, token: Option<String>) -> Result<(), String> {
    if let Some(t) = token {
        crate::config::set_token(&t)?;
    }

    let exe = get_current_exe_path()?;
    let selected = resolve_manager(manager);

    println!("Installing background service via {:?}...", selected);

    match selected {
        InitManager::Launchd => launchd::install(&exe),
        InitManager::Systemd => systemd::install(&exe),
        InitManager::Openrc => openrc::install(&exe),
        InitManager::Windows => windows::install(&exe),
        InitManager::Auto => unreachable!(),
    }
}

pub fn start(manager: InitManager) -> Result<(), String> {
    match resolve_manager(manager) {
        InitManager::Launchd => launchd::start(),
        InitManager::Systemd => systemd::start(),
        InitManager::Openrc => openrc::start(),
        InitManager::Windows => windows::start(),
        InitManager::Auto => unreachable!(),
    }
}

pub fn stop(manager: InitManager) -> Result<(), String> {
    match resolve_manager(manager) {
        InitManager::Launchd => launchd::stop(),
        InitManager::Systemd => systemd::stop(),
        InitManager::Openrc => openrc::stop(),
        InitManager::Windows => windows::stop(),
        InitManager::Auto => unreachable!(),
    }
}

pub fn status(manager: InitManager) -> Result<String, String> {
    match resolve_manager(manager) {
        InitManager::Launchd => launchd::status(),
        InitManager::Systemd => systemd::status(),
        InitManager::Openrc => openrc::status(),
        InitManager::Windows => windows::status(),
        InitManager::Auto => unreachable!(),
    }
}

pub fn uninstall(manager: InitManager) -> Result<(), String> {
    match resolve_manager(manager) {
        InitManager::Launchd => launchd::uninstall(),
        InitManager::Systemd => systemd::uninstall(),
        InitManager::Openrc => openrc::uninstall(),
        InitManager::Windows => windows::uninstall(),
        InitManager::Auto => unreachable!(),
    }
}
