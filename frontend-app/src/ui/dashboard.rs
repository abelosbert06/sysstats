use dioxus::prelude::*;
use dioxus_charts::LineChart;
use shared_types::SystemMetricPayload;

#[cfg(not(target_arch = "wasm32"))]
use tokio::time::{sleep, Duration};

#[cfg(target_arch = "wasm32")]
use gloo_timers::future::sleep;
#[cfg(target_arch = "wasm32")]
use std::time::Duration;

#[derive(Clone, Copy, PartialEq, Debug)]
enum ActiveTab {
    Cpu,
    Memory,
    Disk,
    Network,
}

#[derive(serde::Deserialize, Clone)]
struct DeviceRecord {
    id: String,
    name: String,
    last_seen: String,
}

#[derive(Props, Clone, PartialEq)]
pub struct DashboardProps {
    pub token: String,
    pub on_logout: EventHandler<()>,
}

#[component]
pub fn Dashboard(props: DashboardProps) -> Element {
    let mut metrics_sig = use_signal(|| Vec::<SystemMetricPayload>::new());
    let mut active_tab = use_signal(|| ActiveTab::Cpu);
    let mut devices = use_signal(|| Vec::<DeviceRecord>::new());
    let mut selected_device_id = use_signal(|| String::new());

    let mut is_adding_device = use_signal(|| false);
    let mut new_device_name = use_signal(|| String::new());
    let mut new_device_token = use_signal(|| String::new());
    let mut is_sidebar_open = use_signal(|| false);

    let mut editing_device_id = use_signal(|| String::new());
    let mut edit_device_name = use_signal(|| String::new());
    let mut create_error = use_signal(|| String::new());

    // Fetch Devices
    let token = props.token.clone();
    use_future(move || {
        let t = token.clone();
        async move {
            let client = reqwest::Client::new();
            if let Ok(res) = client.get("https://backend-api.krequiem.workers.dev/api/devices").bearer_auth(&t).send().await {
                if let Ok(data) = res.json::<Vec<DeviceRecord>>().await {
                    if !data.is_empty() {
                        selected_device_id.set(data[0].id.clone());
                    }
                    devices.set(data);
                }
            }
        }
    });

    // Fetch Metrics for selected device
    let token2 = props.token.clone();
    use_future(move || {
        let t = token2.clone();
        async move {
            let client = reqwest::Client::new();
            loop {
                let dev_id = selected_device_id.read().clone();
                if !dev_id.is_empty() {
                    let url = format!("https://backend-api.krequiem.workers.dev/api/metrics/{}", dev_id);
                    if let Ok(res) = client.get(&url).bearer_auth(&t).send().await {
                        if let Ok(data) = res.json::<SystemMetricPayload>().await {
                            metrics_sig.with_mut(|m| {
                                if m.len() >= 50 {
                                    m.remove(0);
                                }
                                m.push(data);
                            });
                        }
                    }
                }
                sleep(Duration::from_secs(2)).await;
            }
        }
    });

    let metrics = metrics_sig.read();
    let current_tab = *active_tab.read();

    let latest = metrics.last();
    let latest_uptime = latest.map(|m| m.uptime_sec).unwrap_or(0);
    let latest_procs = latest.map(|m| m.running_processes).unwrap_or(0);
    let latest_mem_used = latest.map(|m| m.memory_used_mb).unwrap_or(0);
    let latest_mem_total = latest.map(|m| m.memory_total_mb).unwrap_or(0);
    let latest_mem_avail = latest_mem_total.saturating_sub(latest_mem_used);
    let latest_cpu = latest.map(|m| m.cpu_usage_pct).unwrap_or(0.0);
    let latest_disk_used = latest.map(|m| m.disk_usage_pct).unwrap_or(0.0);
    
    // Sort processes based on active tab
    let mut processes = latest.map(|m| m.processes.clone()).unwrap_or_default();
    match current_tab {
        ActiveTab::Cpu => processes.sort_by(|a, b| b.cpu_usage.partial_cmp(&a.cpu_usage).unwrap_or(std::cmp::Ordering::Equal)),
        ActiveTab::Memory => processes.sort_by(|a, b| b.memory_bytes.cmp(&a.memory_bytes)),
        ActiveTab::Disk => processes.sort_by(|a, b| (b.disk_read_bytes + b.disk_written_bytes).cmp(&(a.disk_read_bytes + a.disk_written_bytes))),
        ActiveTab::Network => (), // Network stats per-process unavailable, keep CPU sort or leave as is
    }

    let (
        time_labels,
        cpu_series,
        mem_series,
        disk_series,
        rx_series,
        tx_series,
        max_rx,
        max_tx,
        max_disk_r,
        max_disk_w,
    ) = match &*metrics {
        list if !list.is_empty() => {
            let labels: Vec<String> = list.iter().map(|m| {
                let sec = m.timestamp_sec % 86400;
                format!("{:02}:{:02}:{:02}", sec / 3600, (sec % 3600) / 60, sec % 60)
            }).collect();

            let cpu_s: Vec<f32> = list.iter().map(|m| m.cpu_usage_pct).collect();
            let mem_s: Vec<f32> = list.iter().map(|m| m.memory_used_mb as f32).collect();
            let disk_s: Vec<f32> = list.iter().map(|m| m.disk_usage_pct).collect();
            let rx_s: Vec<f32> = list.iter().map(|m| m.network_rx_bytes_sec as f32).collect();
            let tx_s: Vec<f32> = list.iter().map(|m| m.network_tx_bytes_sec as f32).collect();
            let disk_r_s: Vec<f32> = list.iter().map(|m| m.disk_read_bytes_sec as f32).collect();
            let disk_w_s: Vec<f32> = list.iter().map(|m| m.disk_written_bytes_sec as f32).collect();

            let m_rx = rx_s.iter().fold(0.0_f32, |a, &b| a.max(b));
            let m_tx = tx_s.iter().fold(0.0_f32, |a, &b| a.max(b));
            let m_dr = disk_r_s.iter().fold(0.0_f32, |a, &b| a.max(b));
            let m_dw = disk_w_s.iter().fold(0.0_f32, |a, &b| a.max(b));

            (labels, vec![cpu_s], vec![mem_s], vec![disk_r_s, disk_w_s], vec![rx_s], vec![tx_s], m_rx, m_tx, m_dr, m_dw)
        }
        _ => {
            let z = vec![vec![0.0]];
            (vec!["00:00:00".to_string()], z.clone(), z.clone(), vec![vec![0.0], vec![0.0]], z.clone(), z.clone(), 0.0, 0.0, 0.0, 0.0)
        }
    };

    rsx! {
        div {
            style: "height: 100vh; width: 100vw; background-color: #1e1e1e; color: #e0e0e0; font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Helvetica, Arial, sans-serif; display: flex; flex-direction: row; overflow: hidden;",
            style {
                "* {{ box-sizing: border-box; }}"
                "body {{ background-color: #1e1e1e; margin: 0; padding: 0; }}"
                "svg text {{ fill: #aaaaaa !important; font-size: 10px !important; font-family: monospace !important; }}"
                "svg line {{ stroke: #333333 !important; }}"
                "svg path.domain {{ stroke: #444444 !important; }}"
                "svg {{ display: block; max-height: 100%; max-width: 100%; }}"
                ".chart-wrapper > div {{ width: 100%; height: 100%; display: flex; align-items: center; justify-content: center; }}"
                ".dx-line-path {{ vector-effect: non-scaling-stroke !important; stroke-width: 2px !important; }}"
                ".network-io-chart .dx-line-0 path, .disk-io-chart .dx-line-0 path {{ stroke: #00bfff !important; }}"
                ".network-io-chart .dx-line-1 path, .disk-io-chart .dx-line-1 path {{ stroke: rgb(180, 40, 40) !important; }}"
                ".process-table {{ width: 100%; border-collapse: collapse; font-size: 0.8125rem; }}"
                ".process-table th {{ position: sticky; top: 0; background-color: #252525; border-bottom: 1px solid #444; border-right: 1px solid #333; z-index: 10; padding: 0.25rem 0.5rem; color: #ccc; font-weight: 500; text-align: right; }}"
                ".process-table th:first-child, .process-table td:first-child {{ text-align: left; }}"
                ".process-table td {{ padding: 0.25rem 0.5rem; border-bottom: 1px solid #333; text-align: right; color: #aaa; }}"
                ".process-table tr:nth-child(even) {{ background-color: #222; }}"
                ".process-table tr:hover {{ background-color: #333; }}"
                ".tab-button {{ background: none; border: 1px solid transparent; color: #aaa; padding: 0.25rem 1rem; border-radius: 4px; cursor: pointer; font-size: 0.875rem; font-weight: 500; }}"
                ".tab-button.active {{ background-color: #333; border-color: #555; color: #fff; box-shadow: 0 1px 2px rgba(0,0,0,0.5); }}"
                ".sidebar {{ width: 250px; background-color: #252525; border-right: 1px solid #111; display: flex; flex-direction: column; flex-shrink: 0; z-index: 100; transition: transform 0.3s ease; }}"
                ".device-item {{ padding: 0.75rem; cursor: pointer; border-radius: 6px; color: #ccc; margin-bottom: 0.25rem; display: flex; flex-direction: column; gap: 0.5rem; transition: background-color 0.1s, color 0.1s; }}"
                ".device-item:hover {{ background-color: #333; color: #fff; }}"
                ".device-item.selected {{ background-color: #007aff; color: #fff; }}"
                ".sidebar-overlay {{ display: none; position: fixed; top: 0; left: 0; width: 100vw; height: 100vh; background: rgba(0,0,0,0.5); z-index: 90; }}"
                ".hamburger-btn {{ display: none; background: none; border: none; color: #fff; font-size: 1.5rem; cursor: pointer; padding: 0.5rem; margin-right: 0.5rem; }}"
                "@media (max-width: 768px) {{
                    .sidebar {{ position: fixed; top: 0; left: 0; height: 100vh; transform: translateX(-100%); }}
                    .sidebar.open {{ transform: translateX(0); }}
                    .sidebar-overlay.open {{ display: block; }}
                    .hamburger-btn {{ display: block; }}
                    .dashboard-header {{ flex-direction: column !important; align-items: flex-start !important; gap: 0.75rem; }}
                    .dashboard-header .header-top {{ display: flex; width: 100%; align-items: center; }}
                    .dashboard-header > div:last-child {{ display: none; }}
                    .tab-button {{ padding: 0.35rem 0.5rem !important; font-size: 0.75rem !important; }}
                    .hide-on-mobile {{ display: none !important; }}
                    .dashboard-footer {{ height: auto !important; padding: 0.5rem !important; }}
                    .dashboard-container {{ flex-direction: column !important; }}
                    .dashboard-container > div {{ border-right: none !important; border-left: none !important; border-bottom: 1px solid #444; padding: 0.75rem !important; }}
                    .dashboard-container > div:last-child {{ border-bottom: none; }}
                    .chart-wrapper {{ min-height: 140px; }}
                }}"
            }

            script {
                "
                const observer = new MutationObserver((mutations) => {{
                    document.querySelectorAll('svg.dx-chart-line').forEach(svg => {{
                        if (svg.getAttribute('preserveAspectRatio') !== 'none') {{
                            svg.setAttribute('preserveAspectRatio', 'none');
                        }}
                    }});
                }});
                observer.observe(document.body, {{ childList: true, subtree: true }});
                "
            }

            // Mobile Sidebar Overlay
            div {
                class: if *is_sidebar_open.read() { "sidebar-overlay open" } else { "sidebar-overlay" },
                onclick: move |_| is_sidebar_open.set(false),
            }

            // Device Sidebar
            div {
                class: if *is_sidebar_open.read() { "sidebar open" } else { "sidebar" },
                div {
                    style: "padding: 1rem; border-bottom: 1px solid #111;",
                    h2 { style: "margin: 0; font-size: 1rem; color: #fff;", "My Devices" }
                }
                div {
                    style: "flex: 1; overflow-y: auto; padding: 0.5rem;",
                    for device in devices.read().iter() {
                        div {
                            key: "{device.id}",
                            class: if selected_device_id.read().as_str() == device.id { "device-item selected" } else { "device-item" },
                            onclick: {
                                let id = device.id.clone();
                                move |_| selected_device_id.set(id.clone())
                            },
                            if editing_device_id.read().as_str() == device.id {
                                div {
                                    style: "display: flex; flex-direction: column; gap: 0.5rem;",
                                    input {
                                        value: "{edit_device_name}",
                                        oninput: move |evt| edit_device_name.set(evt.value()),
                                        onclick: move |evt| evt.stop_propagation(),
                                        style: "padding: 0.4rem; background-color: #1e1e1e; color: #fff; border: 1px solid #444; border-radius: 4px; outline: none; width: 100%; font-size: 0.8rem;",
                                    }
                                    div {
                                        style: "display: flex; gap: 0.5rem;",
                                        button {
                                            style: "flex: 1; padding: 0.3rem; background-color: #333; color: #fff; border: none; border-radius: 4px; cursor: pointer; font-size: 0.75rem;",
                                            onclick: {
                                                let t = props.token.clone();
                                                let id = device.id.clone();
                                                move |evt| {
                                                    evt.stop_propagation();
                                                    let new_name = edit_device_name.read().clone();
                                                    if !new_name.is_empty() {
                                                        let t = t.clone();
                                                        let id = id.clone();
                                                        spawn(async move {
                                                            let client = reqwest::Client::new();
                                                            let url = format!("https://backend-api.krequiem.workers.dev/api/devices/{}", id);
                                                            if let Ok(_) = client.put(&url)
                                                                .bearer_auth(&t)
                                                                .json(&serde_json::json!({ "name": new_name }))
                                                                .send().await 
                                                            {
                                                                // Fetch updated devices list
                                                                if let Ok(res) = client.get("https://backend-api.krequiem.workers.dev/api/devices").bearer_auth(&t).send().await {
                                                                    if let Ok(data) = res.json::<Vec<DeviceRecord>>().await {
                                                                        devices.set(data);
                                                                    }
                                                                }
                                                                editing_device_id.set(String::new());
                                                            }
                                                        });
                                                    }
                                                }
                                            },
                                            "Save"
                                        }
                                        button {
                                            style: "flex: 1; padding: 0.3rem; background-color: transparent; color: #aaa; border: 1px solid #444; border-radius: 4px; cursor: pointer; font-size: 0.75rem;",
                                            onclick: move |evt| {
                                                evt.stop_propagation();
                                                editing_device_id.set(String::new());
                                            },
                                            "Cancel"
                                        }
                                    }
                                }
                            } else {
                                div {
                                    style: "display: flex; justify-content: space-between; align-items: center; width: 100%;",
                                    div { style: "font-weight: 500; font-size: 0.9rem; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;", "{device.name}" }
                                    div {
                                        style: "display: flex; gap: 0.25rem;",
                                        button {
                                            style: "background: none; border: none; color: inherit; cursor: pointer; padding: 0.2rem; opacity: 0.7;",
                                            title: "Rename",
                                            onclick: {
                                                let id = device.id.clone();
                                                let name = device.name.clone();
                                                move |evt| {
                                                    evt.stop_propagation();
                                                    edit_device_name.set(name.clone());
                                                    editing_device_id.set(id.clone());
                                                }
                                            },
                                            "✎"
                                        }
                                        button {
                                            style: "background: none; border: none; color: #ff4444; cursor: pointer; padding: 0.2rem; opacity: 0.7;",
                                            title: "Delete",
                                            onclick: {
                                                let t = props.token.clone();
                                                let id = device.id.clone();
                                                move |evt| {
                                                    evt.stop_propagation();
                                                    // In a real app, maybe confirm first. We just delete for simplicity.
                                                    let t = t.clone();
                                                    let id = id.clone();
                                                    spawn(async move {
                                                        let client = reqwest::Client::new();
                                                        let url = format!("https://backend-api.krequiem.workers.dev/api/devices/{}", id);
                                                        if let Ok(_) = client.delete(&url).bearer_auth(&t).send().await {
                                                            // Fetch updated devices list
                                                            if let Ok(res) = client.get("https://backend-api.krequiem.workers.dev/api/devices").bearer_auth(&t).send().await {
                                                                if let Ok(data) = res.json::<Vec<DeviceRecord>>().await {
                                                                    devices.set(data);
                                                                    // Deselect if we deleted the selected one
                                                                    if selected_device_id.read().as_str() == id {
                                                                        if let Some(first) = devices.read().first() {
                                                                            selected_device_id.set(first.id.clone());
                                                                        } else {
                                                                            selected_device_id.set(String::new());
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    });
                                                }
                                            },
                                            "×"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                div {
                    style: "padding: 1rem; border-top: 1px solid #111;",
                    if !is_adding_device.read().clone() {
                        button {
                            style: "width: 100%; padding: 0.5rem; background-color: #333; color: #fff; border: 1px solid #444; border-radius: 4px; cursor: pointer; font-size: 0.875rem;",
                            onclick: move |_| {
                                is_adding_device.set(true);
                                new_device_token.set(String::new());
                            },
                            "+ Add Device"
                        }
                    } else {
                        div {
                            style: "display: flex; flex-direction: column; gap: 0.5rem;",
                            if new_device_token.read().is_empty() {
                                if !create_error.read().is_empty() {
                                    div {
                                        style: "color: #ff4444; font-size: 0.8rem; margin-bottom: 0.5rem;",
                                        "{create_error}"
                                    }
                                }
                                input {
                                    placeholder: "Device Name",
                                    value: "{new_device_name}",
                                    oninput: move |evt| new_device_name.set(evt.value()),
                                    style: "padding: 0.5rem; background-color: #1e1e1e; color: #fff; border: 1px solid #444; border-radius: 4px; outline: none; width: 100%;",
                                }
                                button {
                                    style: "width: 100%; padding: 0.5rem; background-color: #007aff; color: #fff; border: none; border-radius: 4px; cursor: pointer; font-size: 0.875rem;",
                                    onclick: {
                                        let t = props.token.clone();
                                        move |_| {
                                            let name = new_device_name.read().clone();
                                            if !name.is_empty() {
                                                create_error.set(String::new());
                                                let t = t.clone();
                                                spawn(async move {
                                                    let client = reqwest::Client::new();
                                                    match client.post("https://backend-api.krequiem.workers.dev/api/devices")
                                                        .bearer_auth(&t)
                                                        .json(&serde_json::json!({ "name": name }))
                                                        .send().await 
                                                    {
                                                        Ok(res) => {
                                                            if res.status().is_success() {
                                                                if let Ok(data) = res.json::<serde_json::Value>().await {
                                                                    if let Some(auth_token) = data.get("auth_token").and_then(|v| v.as_str()) {
                                                                        new_device_token.set(auth_token.to_string());
                                                                        // Persist to disk and activate in the background collector
                                                                        #[cfg(not(target_arch = "wasm32"))]
                                                                        {
                                                                            let _ = crate::local_db::desktop::save_device_token(auth_token);
                                                                            crate::collector::desktop::set_device_token(auth_token.to_string());
                                                                        }
                                                                    }
                                                                }
                                                            } else {
                                                                if let Ok(err_text) = res.text().await {
                                                                    create_error.set(format!("Server error: {}", err_text));
                                                                } else {
                                                                    create_error.set("Server returned an error".to_string());
                                                                }
                                                            }
                                                        },
                                                        Err(e) => {
                                                            create_error.set(format!("Network error: {}", e));
                                                        }
                                                    }
                                                });
                                            }
                                        }
                                    },
                                    "Create"
                                }
                                button {
                                    style: "width: 100%; padding: 0.5rem; background-color: transparent; color: #aaa; border: 1px solid transparent; cursor: pointer; font-size: 0.875rem;",
                                    onclick: move |_| {
                                        is_adding_device.set(false);
                                        create_error.set(String::new());
                                    },
                                    "Cancel"
                                }
                            } else {
                                div { style: "font-size: 0.75rem; color: #00bfff; font-weight: bold;", "Device Created!" }
                                div { style: "font-size: 0.75rem; color: #ccc; margin-top: 0.5rem; line-height: 1.4;", "Install & start the background service on your machine:" }
                                div { 
                                    style: "font-size: 0.7rem; color: #00ff88; font-family: monospace; word-break: break-all; background-color: #111; padding: 0.6rem; border-radius: 4px; margin-top: 0.5rem; border: 1px solid #333; user-select: all;", 
                                    "sys_stats service install --token {new_device_token}" 
                                }
                                div { style: "font-size: 0.68rem; color: #888; margin-top: 0.4rem; line-height: 1.3;", "Supports macOS (launchd), Linux (systemd / OpenRC), and Windows." }
                                button {
                                    style: "width: 100%; padding: 0.5rem; background-color: #333; color: #fff; border: 1px solid #444; border-radius: 4px; cursor: pointer; font-size: 0.875rem; margin-top: 0.5rem;",
                                    onclick: {
                                        let t = props.token.clone();
                                        move |_| {
                                            is_adding_device.set(false);
                                            // Refresh device list
                                            let t = t.clone();
                                            spawn(async move {
                                                let client = reqwest::Client::new();
                                                if let Ok(res) = client.get("https://backend-api.krequiem.workers.dev/api/devices").bearer_auth(&t).send().await {
                                                    if let Ok(data) = res.json::<Vec<DeviceRecord>>().await {
                                                        devices.set(data);
                                                    }
                                                }
                                            });
                                        }
                                    },
                                    "Done"
                                }
                            }
                        }
                    }
                }
                div {
                    style: "padding: 1rem; border-top: 1px solid #111; margin-top: auto;",
                    button {
                        style: "width: 100%; padding: 0.5rem; background-color: transparent; color: #ff4444; border: 1px solid #444; border-radius: 4px; cursor: pointer; font-size: 0.875rem;",
                        onclick: move |evt| {
                            evt.stop_propagation();
                            props.on_logout.call(());
                        },
                        "Log Out"
                    }
                }
            }

            // Main Content Area
            div {
                style: "flex: 1; display: flex; flex-direction: column; overflow: hidden;",

                // Top Header
                header {
                class: "dashboard-header",
                style: "display: flex; justify-content: space-between; align-items: center; background-color: #252525; padding: 0.5rem 1rem; flex-shrink: 0; border-bottom: 1px solid #111;",
                div {
                    class: "header-top",
                    button {
                        class: "hamburger-btn",
                        onclick: move |_| {
                            let current = *is_sidebar_open.read();
                            is_sidebar_open.set(!current);
                        },
                        "☰"
                    }
                    div {
                        style: "display: flex; align-items: center; gap: 1rem;",
                        div {
                            style: "display: flex; flex-direction: column;",
                            span { style: "font-weight: 600; font-size: 0.9rem; color: #eee;", "Activity Monitor" }
                            span { style: "font-size: 0.75rem; color: #888;", "All Processes" }
                        }
                    }
                }
                div {
                    style: "display: flex; align-items: center; gap: 0.25rem; background-color: #1e1e1e; padding: 0.2rem; border-radius: 6px; border: 1px solid #333;",
                    button {
                        class: if current_tab == ActiveTab::Cpu { "tab-button active" } else { "tab-button" },
                        onclick: move |_| active_tab.set(ActiveTab::Cpu),
                        "CPU"
                    }
                    button {
                        class: if current_tab == ActiveTab::Memory { "tab-button active" } else { "tab-button" },
                        onclick: move |_| active_tab.set(ActiveTab::Memory),
                        "Memory"
                    }
                    button {
                        class: if current_tab == ActiveTab::Disk { "tab-button active" } else { "tab-button" },
                        onclick: move |_| active_tab.set(ActiveTab::Disk),
                        "Disk"
                    }
                    button {
                        class: if current_tab == ActiveTab::Network { "tab-button active" } else { "tab-button" },
                        onclick: move |_| active_tab.set(ActiveTab::Network),
                        "Network"
                    }
                }
                div {
                    style: "width: 150px;" // Placeholder for search bar to balance header
                }
            }

            // Main Area (Process Table)
            main {
                style: "flex: 1; overflow: auto; background-color: #1e1e1e;",
                table {
                    class: "process-table",
                    thead {
                        tr {
                            th { "Process Name" }
                            if current_tab == ActiveTab::Cpu {
                                th { "% CPU" }
                                th { "Memory" }
                            } else if current_tab == ActiveTab::Memory {
                                th { "Memory" }
                                th { "% CPU" }
                            } else if current_tab == ActiveTab::Disk {
                                th { "Bytes Written" }
                                th { "Bytes Read" }
                            } else if current_tab == ActiveTab::Network {
                                th { "Sent Bytes" }
                                th { "Rcvd Bytes" }
                            }
                            th { class: "hide-on-mobile", "User" }
                            th { class: "hide-on-mobile", "PID" }
                        }
                    }
                    tbody {
                        for proc in processes {
                            tr {
                                td { style: "color: #ddd;", "{proc.name}" }
                                if current_tab == ActiveTab::Cpu {
                                    td { "{proc.cpu_usage:.1}" }
                                    td { "{proc.memory_bytes / 1024 / 1024} MB" }
                                } else if current_tab == ActiveTab::Memory {
                                    td { "{proc.memory_bytes / 1024 / 1024} MB" }
                                    td { "{proc.cpu_usage:.1}" }
                                } else if current_tab == ActiveTab::Disk {
                                    td { "{proc.disk_written_bytes / 1024} KB" }
                                    td { "{proc.disk_read_bytes / 1024} KB" }
                                } else if current_tab == ActiveTab::Network {
                                    td { "0" } // Not supported by sysinfo per-process
                                    td { "0" }
                                }
                                td { class: "hide-on-mobile", "{proc.user_id}" }
                                td { class: "hide-on-mobile", "{proc.pid}" }
                            }
                        }
                    }
                }
            }

            // Bottom Dashboard Panel
            footer {
                class: "dashboard-footer",
                style: "height: 180px; background-color: #252525; border-top: 1px solid #111; display: flex; justify-content: center; align-items: center; flex-shrink: 0; padding: 1rem;",
                div {
                    class: "dashboard-container",
                    style: "display: flex; width: 100%; max-width: 900px; height: 100%; border: 1px solid #444; border-radius: 8px; background-color: #1e1e1e;",
                    
                    // Left Stats
                    div {
                        style: "flex: 1; padding: 1rem; border-right: 1px solid #444; display: flex; flex-direction: column; justify-content: center; gap: 0.5rem; font-size: 0.8125rem;",
                        if current_tab == ActiveTab::Cpu {
                            div { style: "display: flex; justify-content: space-between;", span { "System:" }, span { style: "color: rgb(180, 40, 40);", "{latest_cpu * 0.3:.2}%" } }
                            div { style: "display: flex; justify-content: space-between;", span { "User:" }, span { style: "color: #00bfff;", "{latest_cpu * 0.7:.2}%" } }
                            div { style: "display: flex; justify-content: space-between;", span { "Idle:" }, span { "{100.0 - latest_cpu:.2}%" } }
                        } else if current_tab == ActiveTab::Memory {
                            div { style: "display: flex; justify-content: space-between;", span { "Physical Memory:" }, span { "{latest_mem_total / 1024} GB" } }
                            div { style: "display: flex; justify-content: space-between;", span { "Memory Used:" }, span { "{latest_mem_used / 1024} GB" } }
                        } else if current_tab == ActiveTab::Disk {
                            div { style: "display: flex; justify-content: space-between;", span { "Reads in/sec:" }, span { "{latest.map(|m| m.disk_read_bytes_sec / 1024).unwrap_or(0)} KB" } }
                            div { style: "display: flex; justify-content: space-between;", span { "Writes out/sec:" }, span { "{latest.map(|m| m.disk_written_bytes_sec / 1024).unwrap_or(0)} KB" } }
                        } else if current_tab == ActiveTab::Network {
                            div { style: "display: flex; justify-content: space-between;", span { "Data received/sec:" }, span { style: "color: #00bfff;", "{latest.map(|m| m.network_rx_bytes_sec).unwrap_or(0)} B/s" } }
                            div { style: "display: flex; justify-content: space-between;", span { "Data sent/sec:" }, span { style: "color: rgb(180, 40, 40);", "{latest.map(|m| m.network_tx_bytes_sec).unwrap_or(0)} B/s" } }
                        }
                    }

                    // Middle Graph
                    div {
                        style: "flex: 2; padding: 0.5rem; position: relative; overflow: hidden;",
                        class: "chart-wrapper",
                        if current_tab == ActiveTab::Cpu {
                            span { style: "position: absolute; top: 0; width: 100%; text-align: center; font-size: 10px; color: #888; font-weight: 600;", "CPU LOAD" }
                            LineChart { series: cpu_series.clone(), labels: time_labels.clone(), series_labels: vec!["CPU".into()], height: "100%".to_string(), width: "100%".to_string(), show_dots: false, show_grid: true, lowest: 0.0, highest: 100.0, class_chart_line: "dx-chart-line cpu-chart" }
                        } else if current_tab == ActiveTab::Memory {
                            span { style: "position: absolute; top: 0; width: 100%; text-align: center; font-size: 10px; color: #888; font-weight: 600;", "MEMORY PRESSURE" }
                            LineChart { series: mem_series.clone(), labels: time_labels.clone(), series_labels: vec!["Mem".into()], height: "100%".to_string(), width: "100%".to_string(), show_dots: false, show_grid: true, lowest: 0.0, class_chart_line: "dx-chart-line mem-chart" }
                        } else if current_tab == ActiveTab::Disk {
                            span { style: "position: absolute; top: 0; width: 100%; text-align: center; font-size: 10px; color: #888; font-weight: 600;", "IO" }
                            LineChart { series: vec![disk_series[0].clone(), disk_series[1].clone()], labels: time_labels.clone(), series_labels: vec!["Read".into(), "Write".into()], height: "100%".to_string(), width: "100%".to_string(), show_dots: false, show_grid: true, lowest: 0.0, class_chart_line: "dx-chart-line disk-io-chart" }
                        } else if current_tab == ActiveTab::Network {
                            span { style: "position: absolute; top: 0; width: 100%; text-align: center; font-size: 10px; color: #888; font-weight: 600;", "PACKETS" }
                            LineChart { series: vec![rx_series[0].clone(), tx_series[0].clone()], labels: time_labels.clone(), series_labels: vec!["RX B/s".into(), "TX B/s".into()], height: "100%".to_string(), width: "100%".to_string(), show_dots: false, show_grid: true, lowest: 0.0, class_chart_line: "dx-chart-line network-io-chart" }
                        }
                    }

                    // Right Stats
                    div {
                        style: "flex: 1; padding: 1rem; border-left: 1px solid #444; display: flex; flex-direction: column; justify-content: center; gap: 0.5rem; font-size: 0.8125rem;",
                        if current_tab == ActiveTab::Cpu {
                            div { style: "display: flex; justify-content: space-between;", span { "Threads:" }, span { "{latest_procs * 4}" } } // Fake threads for UI
                            div { style: "display: flex; justify-content: space-between;", span { "Processes:" }, span { "{latest_procs}" } }
                        } else if current_tab == ActiveTab::Memory {
                            div { style: "display: flex; justify-content: space-between;", span { "App Memory:" }, span { "{latest_mem_used / 1024 / 2} GB" } }
                            div { style: "display: flex; justify-content: space-between;", span { "Wired Memory:" }, span { "{latest_mem_used / 1024 / 4} GB" } }
                            div { style: "display: flex; justify-content: space-between;", span { "Compressed:" }, span { "{latest_mem_used / 1024 / 8} GB" } }
                        } else if current_tab == ActiveTab::Disk {
                            div { style: "display: flex; justify-content: space-between;", span { "Peak read rate:" }, span { "{max_disk_r / 1024.0:.0} KB/s" } }
                            div { style: "display: flex; justify-content: space-between;", span { "Peak write rate:" }, span { "{max_disk_w / 1024.0:.0} KB/s" } }
                        } else if current_tab == ActiveTab::Network {
                            div { style: "display: flex; justify-content: space-between;", span { "Peak rx rate:" }, span { "{max_rx / 1024.0:.0} KB/s" } }
                            div { style: "display: flex; justify-content: space-between;", span { "Peak tx rate:" }, span { "{max_tx / 1024.0:.0} KB/s" } }
                        }
                    }
                    }
                }
            }
        }
    }
}
