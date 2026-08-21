use dioxus::prelude::*;
use dioxus_charts::LineChart;
use shared_types::{AlertEvent, AlertRule, CreateAlertRuleRequest, SystemMetricPayload};

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

#[derive(serde::Deserialize, Clone, PartialEq)]
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

#[derive(Clone, Debug, PartialEq)]
struct MetricAlertContext {
    metric_type: String,
    display_title: String,
    current_val_str: String,
    unit: String,
    default_threshold: String,
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

    // Alerts and Notifications State
    let mut alert_rules = use_signal(|| Vec::<AlertRule>::new());
    let mut alert_history = use_signal(|| Vec::<AlertEvent>::new());
    let mut is_history_modal_open = use_signal(|| false);
    let mut is_rules_overview_open = use_signal(|| false);
    let mut active_toast = use_signal(|| Option::<AlertEvent>::None);

    // Direct Click-to-Alert state
    let mut target_metric_alert = use_signal(|| Option::<MetricAlertContext>::None);
    let mut target_threshold_input = use_signal(|| String::new());
    let mut target_scope_device = use_signal(|| true);
    let mut target_email = use_signal(|| true);
    let mut target_browser = use_signal(|| true);
    let mut sheet_error = use_signal(|| String::new());

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

    // Poll Alert Rules & History
    let token3 = props.token.clone();
    use_future(move || {
        let t = token3.clone();
        async move {
            let client = reqwest::Client::new();
            loop {
                if let Ok(res) = client.get("https://backend-api.krequiem.workers.dev/api/alerts/rules").bearer_auth(&t).send().await {
                    if let Ok(data) = res.json::<Vec<AlertRule>>().await {
                        alert_rules.set(data);
                    }
                }

                if let Ok(res) = client.get("https://backend-api.krequiem.workers.dev/api/alerts/history").bearer_auth(&t).send().await {
                    if let Ok(events) = res.json::<Vec<AlertEvent>>().await {
                        if let Some(first_unread) = events.iter().find(|e| e.read_at.is_none()) {
                            let current_toast = active_toast.read().clone();
                            if current_toast.as_ref().map(|e| e.id.as_str()) != Some(&first_unread.id) {
                                active_toast.set(Some(first_unread.clone()));
                            }
                        }
                        alert_history.set(events);
                    }
                }

                sleep(Duration::from_secs(5)).await;
            }
        }
    });

    let metrics = metrics_sig.read();
    let current_tab = *active_tab.read();

    let latest = metrics.last();
    let latest_procs = latest.map(|m| m.running_processes).unwrap_or(0);
    let latest_mem_used = latest.map(|m| m.memory_used_mb).unwrap_or(0);
    let latest_mem_total = latest.map(|m| m.memory_total_mb).unwrap_or(0);
    let latest_cpu = latest.map(|m| m.cpu_usage_pct).unwrap_or(0.0);
    let latest_disk_read = latest.map(|m| m.disk_read_bytes_sec / 1024).unwrap_or(0);
    let latest_disk_write = latest.map(|m| m.disk_written_bytes_sec / 1024).unwrap_or(0);
    let latest_net_rx = latest.map(|m| m.network_rx_bytes_sec).unwrap_or(0);
    let latest_net_tx = latest.map(|m| m.network_tx_bytes_sec).unwrap_or(0);

    let empty_vec = Vec::new();
    let processes = latest.map(|m| &m.processes).unwrap_or(&empty_vec);

    let time_labels: Vec<String> = metrics.iter().map(|m| {
        let secs = m.timestamp_sec % 60;
        let mins = (m.timestamp_sec / 60) % 60;
        format!("{:02}:{:02}", mins, secs)
    }).collect();

    let cpu_series: Vec<Vec<f32>> = vec![metrics.iter().map(|m| m.cpu_usage_pct).collect()];
    let mem_series: Vec<Vec<f32>> = vec![metrics.iter().map(|m| {
        if m.memory_total_mb > 0 {
            (m.memory_used_mb as f32 / m.memory_total_mb as f32) * 100.0
        } else {
            0.0
        }
    }).collect()];

    let disk_series: Vec<Vec<f32>> = vec![
        metrics.iter().map(|m| m.disk_read_bytes_sec as f32 / 1024.0).collect(),
        metrics.iter().map(|m| m.disk_written_bytes_sec as f32 / 1024.0).collect(),
    ];

    let rx_series: Vec<Vec<f32>> = vec![metrics.iter().map(|m| m.network_rx_bytes_sec as f32).collect()];
    let tx_series: Vec<Vec<f32>> = vec![metrics.iter().map(|m| m.network_tx_bytes_sec as f32).collect()];

    let max_disk_r = metrics.iter().map(|m| m.disk_read_bytes_sec as f32).fold(0.0f32, f32::max);
    let max_disk_w = metrics.iter().map(|m| m.disk_written_bytes_sec as f32).fold(0.0f32, f32::max);
    let max_rx = metrics.iter().map(|m| m.network_rx_bytes_sec as f32).fold(0.0f32, f32::max);
    let max_tx = metrics.iter().map(|m| m.network_tx_bytes_sec as f32).fold(0.0f32, f32::max);

    let selected_device_name = devices.read().iter()
        .find(|d| d.id == *selected_device_id.read())
        .map(|d| d.name.clone())
        .unwrap_or_else(|| "My Device".to_string());

    let unread_alerts_count = alert_history.read().iter().filter(|e| e.read_at.is_none()).count();

    let is_metric_monitored = |mtype: &str| -> bool {
        let dev_id = selected_device_id.read().clone();
        alert_rules.read().iter().any(|r| r.metric_type == mtype && (r.device_id.as_deref() == Some(&dev_id) || r.device_id.is_none()))
    };

    rsx! {
        div {
            style: "height: 100vh; width: 100vw; background-color: #1e1e1e; color: #e0e0e0; font-family: -apple-system, BlinkMacSystemFont, 'SF Pro Text', 'SF Pro Icons', Roboto, Helvetica, Arial, sans-serif; display: flex; flex-direction: row; overflow: hidden; position: relative;",
            style {
                "* {{ box-sizing: border-box; }}"
                "body {{ background-color: #1e1e1e; margin: 0; padding: 0; }}"
                "svg text {{ fill: #aaaaaa !important; font-size: 10px !important; font-family: -apple-system, monospace !important; }}"
                "svg line {{ stroke: #333333 !important; }}"
                "svg path.domain {{ stroke: #444444 !important; }}"
                "svg:not(.dx-chart-line) {{ display: inline-block; }}"
                "svg.dx-chart-line {{ display: block !important; width: 100% !important; height: 100% !important; max-height: 100% !important; max-width: 100% !important; }}"
                ".chart-wrapper {{ position: relative; width: 100%; height: 100%; overflow: hidden; }}"
                ".chart-wrapper > div {{ width: 100% !important; height: 100% !important; display: flex; align-items: center; justify-content: center; }}"
                ".dx-line-path {{ vector-effect: non-scaling-stroke !important; stroke-width: 2px !important; }}"
                ".network-io-chart .dx-line-0 path, .disk-io-chart .dx-line-0 path {{ stroke: #00bfff !important; }}"
                ".network-io-chart .dx-line-1 path, .disk-io-chart .dx-line-1 path {{ stroke: rgb(180, 40, 40) !important; }}"
                ".process-table {{ width: 100%; border-collapse: collapse; font-size: 0.8125rem; }}"
                ".process-table th {{ position: sticky; top: 0; background-color: #252525; border-bottom: 1px solid #383838; border-right: 1px solid #2e2e2e; z-index: 10; padding: 0.35rem 0.6rem; color: #b0b0b0; font-weight: 500; text-align: right; }}"
                ".process-table th:first-child, .process-table td:first-child {{ text-align: left; }}"
                ".process-table td {{ padding: 0.3rem 0.6rem; border-bottom: 1px solid #282828; text-align: right; color: #a5a5a5; }}"
                ".process-table tr:nth-child(even) {{ background-color: #212121; }}"
                ".process-table tr:hover {{ background-color: #2c2c2e; }}"
                ".tab-button {{ background: none; border: 1px solid transparent; color: #999; padding: 0.25rem 0.9rem; border-radius: 5px; cursor: pointer; font-size: 0.8125rem; font-weight: 500; transition: all 0.15s ease; }}"
                ".tab-button.active {{ background-color: #38383a; border-color: #48484a; color: #fff; box-shadow: 0 1px 3px rgba(0,0,0,0.3); }}"
                ".sidebar {{ width: 240px; background-color: #252527; border-right: 1px solid #1a1a1a; display: flex; flex-direction: column; flex-shrink: 0; z-index: 100; transition: transform 0.3s ease; }}"
                ".device-item {{ padding: 0.65rem 0.75rem; cursor: pointer; border-radius: 6px; color: #bbb; margin-bottom: 0.2rem; display: flex; align-items: center; justify-content: space-between; transition: background-color 0.12s, color 0.12s; font-size: 0.875rem; }}"
                ".device-item:hover {{ background-color: #323234; color: #fff; }}"
                ".device-item.selected {{ background-color: #007aff; color: #fff; font-weight: 500; }}"
                ".stat-row {{ display: flex; justify-content: space-between; align-items: center; padding: 0.25rem 0.4rem; border-radius: 4px; cursor: pointer; transition: background-color 0.12s; }}"
                ".stat-row:hover {{ background-color: #2c2c2e; }}"
                ".stat-row .alert-dot {{ width: 6px; height: 6px; border-radius: 50%; background-color: #007aff; margin-left: 0.35rem; display: inline-block; }}"
                ".sidebar-overlay {{ display: none; position: fixed; top: 0; left: 0; width: 100vw; height: 100vh; background: rgba(0,0,0,0.5); z-index: 90; }}"
                ".hamburger-btn {{ display: none; background: none; border: none; color: #fff; font-size: 1.25rem; cursor: pointer; padding: 0.4rem; margin-right: 0.5rem; }}"
                ".alert-toast {{ position: fixed; top: 1.25rem; right: 1.25rem; z-index: 9999; background: #261818; border: 1px solid #ff453a; border-left: 4px solid #ff453a; color: #fff; padding: 0.85rem 1.15rem; border-radius: 8px; box-shadow: 0 8px 24px rgba(0,0,0,0.5); display: flex; align-items: center; gap: 0.85rem; max-width: 420px; animation: slideIn 0.25s ease; }}"
                "@keyframes slideIn {{ from {{ transform: translateX(100%); opacity: 0; }} to {{ transform: translateX(0); opacity: 1; }} }}"
                "@media (max-width: 768px) {{
                    .sidebar {{ position: fixed; top: 0; left: 0; height: 100vh; transform: translateX(-100%); }}
                    .sidebar.open {{ transform: translateX(0); }}
                    .sidebar-overlay.open {{ display: block; }}
                    .hamburger-btn {{ display: block; }}
                    .dashboard-header {{ flex-direction: column !important; align-items: flex-start !important; gap: 0.75rem; }}
                    .dashboard-header .header-top {{ display: flex; width: 100%; align-items: center; }}
                    .tab-button {{ padding: 0.3rem 0.5rem !important; font-size: 0.75rem !important; }}
                    .hide-on-mobile {{ display: none !important; }}
                    .dashboard-footer {{ height: auto !important; padding: 0.5rem !important; }}
                    .dashboard-container {{ flex-direction: column !important; }}
                    .dashboard-container > div {{ border-right: none !important; border-left: none !important; border-bottom: 1px solid #333; padding: 0.75rem !important; }}
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

            // In-App Alert Toast Notification
            if let Some(toast) = active_toast.read().clone() {
                div {
                    class: "alert-toast",
                    svg {
                        style: "width: 20px; height: 20px; flex-shrink: 0; fill: #ff453a;",
                        view_box: "0 0 24 24",
                        path { d: "M12 2L1 21h22L12 2zm0 3.5L20.3 19H3.7L12 5.5zM11 10v4h2v-4h-2zm0 6v2h2v-2h-2z" }
                    }
                    div {
                        style: "flex: 1;",
                        div { style: "font-size: 0.75rem; font-weight: 600; color: #ff6961; text-transform: uppercase; letter-spacing: 0.5px;", "Threshold Alert" }
                        div { style: "font-size: 0.8125rem; margin-top: 0.15rem; color: #f0f0f0;", "{toast.message}" }
                    }
                    button {
                        style: "background: #333; border: 1px solid #444; color: #eee; border-radius: 4px; padding: 0.25rem 0.5rem; font-size: 0.75rem; cursor: pointer;",
                        onclick: {
                            let t = props.token.clone();
                            let aid = toast.id.clone();
                            move |_| {
                                active_toast.set(None);
                                let t = t.clone();
                                let aid = aid.clone();
                                spawn(async move {
                                    let client = reqwest::Client::new();
                                    let _ = client.post(format!("https://backend-api.krequiem.workers.dev/api/alerts/history/{}/read", aid)).bearer_auth(&t).send().await;
                                });
                            }
                        },
                        "Dismiss"
                    }
                }
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
                    style: "padding: 0.85rem 1rem; border-bottom: 1px solid #1f1f21; display: flex; justify-content: space-between; align-items: center;",
                    div {
                        style: "font-weight: 600; font-size: 0.875rem; color: #f5f5f7; letter-spacing: -0.2px;",
                        "Activity Monitor"
                    }
                    button {
                        style: "background: none; border: none; color: #8e8e93; cursor: pointer; padding: 0.2rem; display: flex; align-items: center;",
                        title: "Log Out",
                        onclick: move |_| props.on_logout.call(()),
                        svg {
                            style: "width: 16px; height: 16px; fill: currentColor;",
                            view_box: "0 0 24 24",
                            path { d: "M16 13v-2H7V8l-5 4 5 4v-3h9zM20 3h-9c-1.1 0-2 .9-2 2v4h2V5h9v14h-9v-4H9v4c0 1.1.9 2 2 2h9c1.1 0 2-.9 2-2V5c0-1.1-.9-2-2-2z" }
                        }
                    }
                }

                div {
                    style: "flex: 1; overflow-y: auto; padding: 0.4rem;",
                    for dev in devices.read().iter() {
                        div {
                            key: "{dev.id}",
                            class: if *selected_device_id.read() == dev.id { "device-item selected" } else { "device-item" },
                            onclick: {
                                let id = dev.id.clone();
                                move |_| {
                                    selected_device_id.set(id.clone());
                                    metrics_sig.set(Vec::new());
                                    is_sidebar_open.set(false);
                                }
                            },
                            span { style: "overflow: hidden; text-overflow: ellipsis; white-space: nowrap;", "{dev.name}" }
                            div {
                                style: "display: flex; gap: 0.2rem; align-items: center;",
                                button {
                                    style: "background: none; border: none; color: inherit; opacity: 0.7; cursor: pointer; padding: 0.2rem; display: flex; align-items: center;",
                                    title: "Rename",
                                    onclick: {
                                        let id = dev.id.clone();
                                        let name = dev.name.clone();
                                        move |evt| {
                                            evt.stop_propagation();
                                            editing_device_id.set(id.clone());
                                            edit_device_name.set(name.clone());
                                        }
                                    },
                                    svg {
                                        style: "width: 13px; height: 13px; fill: currentColor;",
                                        view_box: "0 0 24 24",
                                        path { d: "M3 17.25V21h3.75L17.81 9.94l-3.75-3.75L3 17.25zM20.71 7.04c.39-.39.39-1.02 0-1.41l-2.34-2.34c-.39-.39-1.02-.39-1.41 0l-1.83 1.83 3.75 3.75 1.83-1.83z" }
                                    }
                                }
                                button {
                                    style: "background: none; border: none; color: inherit; opacity: 0.7; cursor: pointer; padding: 0.2rem; display: flex; align-items: center;",
                                    title: "Delete",
                                    onclick: {
                                        let id = dev.id.clone();
                                        let t = props.token.clone();
                                        move |evt| {
                                            evt.stop_propagation();
                                            let id = id.clone();
                                            let t = t.clone();
                                            spawn(async move {
                                                let client = reqwest::Client::new();
                                                let url = format!("https://backend-api.krequiem.workers.dev/api/devices/{}", id);
                                                if let Ok(res) = client.delete(&url).bearer_auth(&t).send().await {
                                                    if res.status().is_success() {
                                                        if let Ok(res2) = client.get("https://backend-api.krequiem.workers.dev/api/devices").bearer_auth(&t).send().await {
                                                            if let Ok(data) = res2.json::<Vec<DeviceRecord>>().await {
                                                                if *selected_device_id.read() == id {
                                                                    if !data.is_empty() {
                                                                        selected_device_id.set(data[0].id.clone());
                                                                    } else {
                                                                        selected_device_id.set(String::new());
                                                                    }
                                                                }
                                                                devices.set(data);
                                                            }
                                                        }
                                                    }
                                                }
                                            });
                                        }
                                    },
                                    svg {
                                        style: "width: 13px; height: 13px; fill: currentColor;",
                                        view_box: "0 0 24 24",
                                        path { d: "M6 19c0 1.1.9 2 2 2h8c1.1 0 2-.9 2-2V7H6v12zM19 4h-3.5l-1-1h-5l-1 1H5v2h14V4z" }
                                    }
                                }
                            }
                        }
                    }
                }

                // Sidebar Footer
                div {
                    style: "padding: 0.75rem; border-top: 1px solid #1f1f21; display: flex; flex-direction: column; gap: 0.4rem;",
                    button {
                        style: "width: 100%; padding: 0.45rem; background-color: #007aff; color: #fff; border: none; border-radius: 5px; cursor: pointer; font-size: 0.8125rem; font-weight: 500;",
                        onclick: move |_| {
                            new_device_name.set(String::new());
                            new_device_token.set(String::new());
                            create_error.set(String::new());
                            is_adding_device.set(true);
                        },
                        "+ Add Device"
                    }
                    button {
                        style: "width: 100%; padding: 0.4rem; background-color: #2c2c2e; color: #aaa; border: 1px solid #3a3a3c; border-radius: 5px; cursor: pointer; font-size: 0.75rem;",
                        onclick: move |_| is_rules_overview_open.set(true),
                        "Manage All Rules"
                    }
                }
            }

            // Main Central Panel
            div {
                style: "flex: 1; display: flex; flex-direction: column; overflow: hidden; background-color: #1e1e1e;",

                // Header
                header {
                    class: "dashboard-header",
                    style: "height: 52px; background-color: #252527; border-bottom: 1px solid #1a1a1a; display: flex; align-items: center; justify-content: space-between; padding: 0 1rem; flex-shrink: 0;",
                    div {
                        class: "header-top",
                        style: "display: flex; align-items: center;",
                        button {
                            class: "hamburger-btn",
                            onclick: move |_| {
                                let cur = *is_sidebar_open.read();
                                is_sidebar_open.set(!cur);
                            },
                            "☰"
                        }
                        div {
                            h2 { style: "margin: 0; font-size: 1rem; font-weight: 600; color: #f5f5f7;", "{selected_device_name}" }
                            span { style: "font-size: 0.7rem; color: #8e8e93;", "Telemetry Monitor" }
                        }
                    }

                    // Segmented Tabs
                    div {
                        style: "display: flex; align-items: center; gap: 0.15rem; background-color: #1c1c1e; padding: 0.15rem; border-radius: 6px; border: 1px solid #2c2c2e;",
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

                    // Header Right Notifications Bell
                    div {
                        style: "display: flex; align-items: center; gap: 0.5rem;",
                        button {
                            style: "background: #1c1c1e; border: 1px solid #333; color: #e0e0e0; border-radius: 5px; padding: 0.35rem 0.6rem; font-size: 0.75rem; cursor: pointer; display: flex; align-items: center; gap: 0.4rem;",
                            title: "Alerts Center",
                            onclick: move |_| is_history_modal_open.set(true),
                            svg {
                                style: "width: 14px; height: 14px; fill: currentColor;",
                                view_box: "0 0 24 24",
                                path { d: "M12 22c1.1 0 2-.9 2-2h-4c0 1.1.9 2 2 2zm6-6v-5c0-3.07-1.63-5.64-4.5-6.32V4c0-.83-.67-1.5-1.5-1.5s-1.5.67-1.5 1.5v.68C7.64 5.36 6 7.92 6 11v5l-2 2v1h16v-1l-2-2z" }
                            }
                            if unread_alerts_count > 0 {
                                span {
                                    style: "background: #ff453a; color: #fff; font-size: 0.65rem; font-weight: 700; padding: 0.05rem 0.35rem; border-radius: 8px;",
                                    "{unread_alerts_count}"
                                }
                            }
                        }
                    }
                }

                // Process Table
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
                                        td { "0" }
                                        td { "0" }
                                    }
                                    td { class: "hide-on-mobile", "{proc.user_id}" }
                                    td { class: "hide-on-mobile", "{proc.pid}" }
                                }
                            }
                        }
                    }
                }

                // Interactive Bottom Stats Panel
                footer {
                    class: "dashboard-footer",
                    style: "height: 180px; background-color: #252527; border-top: 1px solid #1a1a1a; display: flex; justify-content: center; align-items: center; flex-shrink: 0; padding: 0.75rem;",
                    div {
                        class: "dashboard-container",
                        style: "display: flex; width: 100%; max-width: 900px; height: 100%; border: 1px solid #333; border-radius: 8px; background-color: #1e1e1e;",
                        
                        // Left Interactive Stats Column
                        div {
                            style: "flex: 1; padding: 0.75rem; border-right: 1px solid #333; display: flex; flex-direction: column; justify-content: center; gap: 0.35rem; font-size: 0.8125rem;",
                            if current_tab == ActiveTab::Cpu {
                                div {
                                    class: "stat-row",
                                    title: "Click to set alert threshold",
                                    onclick: move |_| {
                                        target_metric_alert.set(Some(MetricAlertContext {
                                            metric_type: "cpu".to_string(),
                                            display_title: "System CPU Usage".to_string(),
                                            current_val_str: format!("{:.2}%", latest_cpu * 0.3),
                                            unit: "%".to_string(),
                                            default_threshold: "80".to_string(),
                                        }));
                                        target_threshold_input.set("80".to_string());
                                        sheet_error.set(String::new());
                                    },
                                    span { "System:" if is_metric_monitored("cpu") { span { class: "alert-dot" } } },
                                    span { style: "color: rgb(180, 40, 40);", "{latest_cpu * 0.3:.2}%" }
                                }
                                div {
                                    class: "stat-row",
                                    title: "Click to set alert threshold",
                                    onclick: move |_| {
                                        target_metric_alert.set(Some(MetricAlertContext {
                                            metric_type: "cpu".to_string(),
                                            display_title: "User CPU Usage".to_string(),
                                            current_val_str: format!("{:.2}%", latest_cpu * 0.7),
                                            unit: "%".to_string(),
                                            default_threshold: "85".to_string(),
                                        }));
                                        target_threshold_input.set("85".to_string());
                                        sheet_error.set(String::new());
                                    },
                                    span { "User:" },
                                    span { style: "color: #00bfff;", "{latest_cpu * 0.7:.2}%" }
                                }
                                div {
                                    class: "stat-row",
                                    title: "Click to set alert threshold",
                                    onclick: move |_| {
                                        target_metric_alert.set(Some(MetricAlertContext {
                                            metric_type: "cpu".to_string(),
                                            display_title: "Total CPU Load".to_string(),
                                            current_val_str: format!("{:.2}%", latest_cpu),
                                            unit: "%".to_string(),
                                            default_threshold: "90".to_string(),
                                        }));
                                        target_threshold_input.set("90".to_string());
                                        sheet_error.set(String::new());
                                    },
                                    span { "Idle:" },
                                    span { "{100.0 - latest_cpu:.2}%" }
                                }
                            } else if current_tab == ActiveTab::Memory {
                                div {
                                    class: "stat-row",
                                    span { "Physical Memory:" },
                                    span { "{latest_mem_total / 1024} GB" }
                                }
                                div {
                                    class: "stat-row",
                                    title: "Click to set alert threshold",
                                    onclick: move |_| {
                                        target_metric_alert.set(Some(MetricAlertContext {
                                            metric_type: "memory".to_string(),
                                            display_title: "Memory Pressure".to_string(),
                                            current_val_str: format!("{} GB", latest_mem_used / 1024),
                                            unit: "%".to_string(),
                                            default_threshold: "90".to_string(),
                                        }));
                                        target_threshold_input.set("90".to_string());
                                        sheet_error.set(String::new());
                                    },
                                    span { "Memory Used:" if is_metric_monitored("memory") { span { class: "alert-dot" } } },
                                    span { "{latest_mem_used / 1024} GB" }
                                }
                            } else if current_tab == ActiveTab::Disk {
                                div {
                                    class: "stat-row",
                                    title: "Click to set alert threshold",
                                    onclick: move |_| {
                                        target_metric_alert.set(Some(MetricAlertContext {
                                            metric_type: "disk".to_string(),
                                            display_title: "Disk Read Rate".to_string(),
                                            current_val_str: format!("{} KB/s", latest_disk_read),
                                            unit: "KB/s".to_string(),
                                            default_threshold: "50000".to_string(),
                                        }));
                                        target_threshold_input.set("50000".to_string());
                                        sheet_error.set(String::new());
                                    },
                                    span { "Reads in/sec:" if is_metric_monitored("disk") { span { class: "alert-dot" } } },
                                    span { "{latest_disk_read} KB" }
                                }
                                div {
                                    class: "stat-row",
                                    title: "Click to set alert threshold",
                                    onclick: move |_| {
                                        target_metric_alert.set(Some(MetricAlertContext {
                                            metric_type: "disk".to_string(),
                                            display_title: "Disk Write Rate".to_string(),
                                            current_val_str: format!("{} KB/s", latest_disk_write),
                                            unit: "KB/s".to_string(),
                                            default_threshold: "50000".to_string(),
                                        }));
                                        target_threshold_input.set("50000".to_string());
                                        sheet_error.set(String::new());
                                    },
                                    span { "Writes out/sec:" },
                                    span { "{latest_disk_write} KB" }
                                }
                            } else if current_tab == ActiveTab::Network {
                                div {
                                    class: "stat-row",
                                    span { "Data received/sec:" },
                                    span { style: "color: #00bfff;", "{latest_net_rx} B/s" }
                                }
                                div {
                                    class: "stat-row",
                                    span { "Data sent/sec:" },
                                    span { style: "color: rgb(180, 40, 40);", "{latest_net_tx} B/s" }
                                }
                            }
                        }

                        // Middle Chart
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

                        // Right Interactive Stats Column
                        div {
                            style: "flex: 1; padding: 0.75rem; border-left: 1px solid #333; display: flex; flex-direction: column; justify-content: center; gap: 0.35rem; font-size: 0.8125rem;",
                            if current_tab == ActiveTab::Cpu {
                                div { style: "display: flex; justify-content: space-between;", span { "Threads:" }, span { "{latest_procs * 4}" } }
                                div {
                                    class: "stat-row",
                                    title: "Click to set alert threshold",
                                    onclick: move |_| {
                                        target_metric_alert.set(Some(MetricAlertContext {
                                            metric_type: "cpu".to_string(),
                                            display_title: "Active Processes".to_string(),
                                            current_val_str: format!("{}", latest_procs),
                                            unit: "Count".to_string(),
                                            default_threshold: "1000".to_string(),
                                        }));
                                        target_threshold_input.set("1000".to_string());
                                        sheet_error.set(String::new());
                                    },
                                    span { "Processes:" },
                                    span { "{latest_procs}" }
                                }
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

            // Modal: Direct Metric Alert Sheet (macOS Popover Sheet)
            if let Some(target) = target_metric_alert.read().clone() {
                div {
                    style: "position: fixed; top: 0; left: 0; width: 100vw; height: 100vh; background-color: rgba(0,0,0,0.6); display: flex; align-items: center; justify-content: center; z-index: 1000; animation: fadeIn 0.15s ease;",
                    div {
                        style: "background-color: #28282a; padding: 1.75rem; border-radius: 10px; border: 1px solid #3a3a3c; width: 100%; max-width: 420px; box-shadow: 0 16px 36px rgba(0,0,0,0.6); display: flex; flex-direction: column; gap: 1.25rem;",
                        div {
                            style: "display: flex; justify-content: space-between; align-items: flex-start;",
                            div {
                                h3 { style: "margin: 0; color: #fff; font-size: 1.05rem; font-weight: 600;", "Set Alert: {target.display_title}" }
                                div { style: "font-size: 0.75rem; color: #8e8e93; margin-top: 0.2rem;", "Current value: {target.current_val_str}" }
                            }
                            button {
                                style: "background: none; border: none; color: #8e8e93; cursor: pointer; padding: 0.2rem;",
                                onclick: move |_| target_metric_alert.set(None),
                                svg {
                                    style: "width: 14px; height: 14px; fill: currentColor;",
                                    view_box: "0 0 24 24",
                                    path { d: "M19 6.41L17.59 5 12 10.59 6.41 5 5 6.41 10.59 12 5 17.59 6.41 19 12 13.41 17.59 19 19 17.59 13.41 12z" }
                                }
                            }
                        }

                        div {
                            style: "display: flex; flex-direction: column; gap: 0.85rem;",
                            div {
                                label { style: "font-size: 0.75rem; color: #bbb; font-weight: 500;", "Trigger notification when value exceeds ({target.unit}):" }
                                input {
                                    type: "number",
                                    style: "width: 100%; background: #1c1c1e; border: 1px solid #3a3a3c; color: #fff; padding: 0.5rem; border-radius: 5px; font-size: 0.9rem; margin-top: 0.3rem;",
                                    value: "{target_threshold_input}",
                                    oninput: move |evt| target_threshold_input.set(evt.value()),
                                }
                            }

                            div {
                                label { style: "font-size: 0.75rem; color: #bbb; font-weight: 500;", "Scope:" }
                                div {
                                    style: "display: flex; gap: 0.5rem; margin-top: 0.3rem;",
                                    button {
                                        style: format!(
                                            "flex: 1; padding: 0.4rem; border-radius: 5px; font-size: 0.75rem; cursor: pointer; border: 1px solid {}; background: {}; color: {};",
                                            if *target_scope_device.read() { "#007aff" } else { "#3a3a3c" },
                                            if *target_scope_device.read() { "#007aff" } else { "#1c1c1e" },
                                            if *target_scope_device.read() { "#fff" } else { "#aaa" }
                                        ),
                                        onclick: move |_| target_scope_device.set(true),
                                        "This Device Only"
                                    }
                                    button {
                                        style: format!(
                                            "flex: 1; padding: 0.4rem; border-radius: 5px; font-size: 0.75rem; cursor: pointer; border: 1px solid {}; background: {}; color: {};",
                                            if !*target_scope_device.read() { "#007aff" } else { "#3a3a3c" },
                                            if !*target_scope_device.read() { "#007aff" } else { "#1c1c1e" },
                                            if !*target_scope_device.read() { "#fff" } else { "#aaa" }
                                        ),
                                        onclick: move |_| target_scope_device.set(false),
                                        "All Devices"
                                    }
                                }
                            }

                            div {
                                style: "display: flex; gap: 1rem; align-items: center; margin-top: 0.25rem;",
                                label {
                                    style: "font-size: 0.75rem; color: #bbb; display: flex; align-items: center; gap: 0.35rem; cursor: pointer;",
                                    input {
                                        type: "checkbox",
                                        checked: *target_email.read(),
                                        onchange: move |_| {
                                            let cur = *target_email.read();
                                            target_email.set(!cur);
                                        }
                                    }
                                    "Email Alert"
                                }
                                label {
                                    style: "font-size: 0.75rem; color: #bbb; display: flex; align-items: center; gap: 0.35rem; cursor: pointer;",
                                    input {
                                        type: "checkbox",
                                        checked: *target_browser.read(),
                                        onchange: move |_| {
                                            let cur = *target_browser.read();
                                            target_browser.set(!cur);
                                        }
                                    }
                                    "In-App Toast Alert"
                                }
                            }

                            if !sheet_error.read().is_empty() {
                                div { style: "color: #ff453a; font-size: 0.75rem;", "{sheet_error}" }
                            }
                        }

                        div {
                            style: "display: flex; justify-content: flex-end; gap: 0.5rem; border-top: 1px solid #333; padding-top: 1rem;",
                            button {
                                style: "padding: 0.45rem 0.9rem; background: #3a3a3c; color: #eee; border: none; border-radius: 5px; cursor: pointer; font-size: 0.8125rem;",
                                onclick: move |_| target_metric_alert.set(None),
                                "Cancel"
                            }
                            button {
                                style: "padding: 0.45rem 1rem; background: #007aff; color: #fff; border: none; border-radius: 5px; cursor: pointer; font-size: 0.8125rem; font-weight: 500;",
                                onclick: {
                                    let t = props.token.clone();
                                    move |_| {
                                        let thresh: f32 = match target_threshold_input.read().parse() {
                                            Ok(v) => v,
                                            Err(_) => {
                                                sheet_error.set("Please enter a valid number".to_string());
                                                return;
                                            }
                                        };
                                        let mtype = target.metric_type.clone();
                                        let dev_opt = if *target_scope_device.read() {
                                            let cur_dev = selected_device_id.read().clone();
                                            if cur_dev.is_empty() { None } else { Some(cur_dev) }
                                        } else {
                                            None
                                        };

                                        let req = CreateAlertRuleRequest {
                                            device_id: dev_opt,
                                            metric_type: mtype,
                                            threshold_value: thresh,
                                            cooldown_seconds: Some(900),
                                            notify_email: Some(*target_email.read()),
                                            notify_browser: Some(*target_browser.read()),
                                        };

                                        let t = t.clone();
                                        spawn(async move {
                                            let client = reqwest::Client::new();
                                            let res = client.post("https://backend-api.krequiem.workers.dev/api/alerts/rules")
                                                .bearer_auth(&t)
                                                .json(&req)
                                                .send()
                                                .await;

                                            match res {
                                                Ok(resp) => {
                                                    if resp.status().is_success() {
                                                        target_metric_alert.set(None);
                                                        if let Ok(res2) = client.get("https://backend-api.krequiem.workers.dev/api/alerts/rules").bearer_auth(&t).send().await {
                                                            if let Ok(data) = res2.json::<Vec<AlertRule>>().await {
                                                                alert_rules.set(data);
                                                            }
                                                        }
                                                    } else {
                                                        let txt = resp.text().await.unwrap_or_else(|_| "Unknown server error".to_string());
                                                        sheet_error.set(format!("Server error: {}", txt));
                                                    }
                                                }
                                                Err(e) => {
                                                    sheet_error.set(format!("Network error: {}", e));
                                                }
                                            }
                                        });
                                    }
                                },
                                "Set Alert"
                            }
                        }
                    }
                }
            }

            // Modal: Add Device
            if *is_adding_device.read() {
                div {
                    style: "position: fixed; top: 0; left: 0; width: 100vw; height: 100vh; background-color: rgba(0,0,0,0.6); display: flex; align-items: center; justify-content: center; z-index: 1000;",
                    div {
                        style: "background-color: #28282a; padding: 1.75rem; border-radius: 10px; border: 1px solid #3a3a3c; width: 100%; max-width: 440px; display: flex; flex-direction: column; gap: 1rem;",
                        h3 { style: "margin: 0; color: #fff; font-size: 1.05rem;", "Add New Device" }
                        if new_device_token.read().is_empty() {
                            input {
                                type: "text",
                                placeholder: "Device Name (e.g. Work MacBook)",
                                value: "{new_device_name}",
                                oninput: move |evt| new_device_name.set(evt.value()),
                                style: "background-color: #1c1c1e; border: 1px solid #3a3a3c; color: #fff; padding: 0.5rem; border-radius: 5px; font-size: 0.875rem;",
                            }
                            if !create_error.read().is_empty() {
                                div { style: "color: #ff453a; font-size: 0.75rem;", "{create_error}" }
                            }
                            div {
                                style: "display: flex; justify-content: flex-end; gap: 0.5rem; margin-top: 0.5rem;",
                                button {
                                    style: "padding: 0.45rem 0.9rem; background-color: transparent; color: #aaa; border: 1px solid transparent; cursor: pointer; font-size: 0.8125rem;",
                                    onclick: move |_| {
                                        is_adding_device.set(false);
                                        create_error.set(String::new());
                                    },
                                    "Cancel"
                                }
                                button {
                                    style: "padding: 0.45rem 1rem; background-color: #007aff; color: #fff; border: none; border-radius: 5px; cursor: pointer; font-size: 0.8125rem; font-weight: 500;",
                                    onclick: {
                                        let t = props.token.clone();
                                        move |_| {
                                            let name = new_device_name.read().clone();
                                            if name.is_empty() {
                                                create_error.set("Name cannot be empty".to_string());
                                                return;
                                            }
                                            let t = t.clone();
                                            spawn(async move {
                                                let client = reqwest::Client::new();
                                                let res = client.post("https://backend-api.krequiem.workers.dev/api/devices")
                                                    .bearer_auth(&t)
                                                    .json(&serde_json::json!({ "name": name }))
                                                    .send()
                                                    .await;
                                                match res {
                                                    Ok(resp) => {
                                                        if resp.status().is_success() {
                                                            if let Ok(data) = resp.json::<serde_json::Value>().await {
                                                                if let Some(tok) = data.get("auth_token").and_then(|v| v.as_str()) {
                                                                    new_device_token.set(tok.to_string());
                                                                }
                                                            }
                                                        } else {
                                                            create_error.set("Failed to create device".to_string());
                                                        }
                                                    }
                                                    Err(e) => create_error.set(e.to_string()),
                                                }
                                            });
                                        }
                                    },
                                    "Create Device"
                                }
                            }
                        } else {
                            div { style: "font-size: 0.8125rem; color: #30d158; font-weight: 600;", "Device Created Successfully" }
                            div { style: "font-size: 0.75rem; color: #bbb; line-height: 1.4;", "Install and run the background service on your machine:" }
                            div { 
                                style: "font-size: 0.75rem; color: #30d158; font-family: monospace; word-break: break-all; background-color: #111; padding: 0.65rem; border-radius: 5px; border: 1px solid #333; user-select: all;", 
                                "sys_stats service install --token {new_device_token}" 
                            }
                            div { style: "font-size: 0.7rem; color: #8e8e93; line-height: 1.3;", "Supports macOS (launchd), Linux (systemd / OpenRC), and Windows." }
                            button {
                                style: "width: 100%; padding: 0.5rem; background-color: #333; color: #fff; border: 1px solid #444; border-radius: 5px; cursor: pointer; font-size: 0.8125rem; margin-top: 0.5rem;",
                                onclick: {
                                    let t = props.token.clone();
                                    move |_| {
                                        is_adding_device.set(false);
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

            // Modal: Rename Device
            if !editing_device_id.read().is_empty() {
                div {
                    style: "position: fixed; top: 0; left: 0; width: 100vw; height: 100vh; background-color: rgba(0,0,0,0.6); display: flex; align-items: center; justify-content: center; z-index: 1000;",
                    div {
                        style: "background-color: #28282a; padding: 1.75rem; border-radius: 10px; border: 1px solid #3a3a3c; width: 100%; max-width: 380px; display: flex; flex-direction: column; gap: 1rem;",
                        h3 { style: "margin: 0; color: #fff; font-size: 1rem;", "Rename Device" }
                        input {
                            type: "text",
                            value: "{edit_device_name}",
                            oninput: move |evt| edit_device_name.set(evt.value()),
                            style: "background-color: #1c1c1e; border: 1px solid #3a3a3c; color: #fff; padding: 0.5rem; border-radius: 5px; font-size: 0.875rem;",
                        }
                        div {
                            style: "display: flex; justify-content: flex-end; gap: 0.5rem;",
                            button {
                                style: "padding: 0.45rem 0.9rem; background-color: transparent; color: #aaa; border: none; cursor: pointer; font-size: 0.8125rem;",
                                onclick: move |_| editing_device_id.set(String::new()),
                                "Cancel"
                            }
                            button {
                                style: "padding: 0.45rem 1rem; background-color: #007aff; color: #fff; border: none; border-radius: 5px; cursor: pointer; font-size: 0.8125rem; font-weight: 500;",
                                onclick: {
                                    let id = editing_device_id.read().clone();
                                    let name = edit_device_name.read().clone();
                                    let t = props.token.clone();
                                    move |_| {
                                        let id = id.clone();
                                        let name = name.clone();
                                        let t = t.clone();
                                        spawn(async move {
                                            let client = reqwest::Client::new();
                                            let url = format!("https://backend-api.krequiem.workers.dev/api/devices/{}", id);
                                            let _ = client.put(&url).bearer_auth(&t).json(&serde_json::json!({ "name": name })).send().await;
                                            if let Ok(res) = client.get("https://backend-api.krequiem.workers.dev/api/devices").bearer_auth(&t).send().await {
                                                if let Ok(data) = res.json::<Vec<DeviceRecord>>().await {
                                                    devices.set(data);
                                                }
                                            }
                                        });
                                        editing_device_id.set(String::new());
                                    }
                                },
                                "Save"
                            }
                        }
                    }
                }
            }

            // Modal: Notification History Feed (macOS Notification Drawer)
            if *is_history_modal_open.read() {
                div {
                    style: "position: fixed; top: 0; left: 0; width: 100vw; height: 100vh; background-color: rgba(0,0,0,0.6); display: flex; align-items: center; justify-content: center; z-index: 1000;",
                    div {
                        style: "background-color: #28282a; padding: 1.75rem; border-radius: 10px; border: 1px solid #3a3a3c; width: 100%; max-width: 480px; max-height: 80vh; display: flex; flex-direction: column; gap: 1rem;",
                        div {
                            style: "display: flex; justify-content: space-between; align-items: center;",
                            h3 { style: "margin: 0; color: #fff; font-size: 1.05rem; font-weight: 600;", "Notifications" }
                            button {
                                style: "background: none; border: none; color: #8e8e93; cursor: pointer; padding: 0.2rem;",
                                onclick: move |_| is_history_modal_open.set(false),
                                svg {
                                    style: "width: 14px; height: 14px; fill: currentColor;",
                                    view_box: "0 0 24 24",
                                    path { d: "M19 6.41L17.59 5 12 10.59 6.41 5 5 6.41 10.59 12 5 17.59 6.41 19 12 13.41 17.59 19 19 17.59 13.41 12z" }
                                }
                            }
                        }

                        div {
                            style: "flex: 1; overflow-y: auto; display: flex; flex-direction: column; gap: 0.4rem;",
                            if alert_history.read().is_empty() {
                                div { style: "font-size: 0.8125rem; color: #666; font-style: italic; text-align: center; padding: 2rem;", "No alert notifications yet." }
                            }
                            for event in alert_history.read().iter() {
                                div {
                                    key: "{event.id}",
                                    style: format!(
                                        "background: {}; border: 1px solid {}; padding: 0.65rem 0.85rem; border-radius: 6px; display: flex; justify-content: space-between; align-items: center;",
                                        if event.read_at.is_none() { "#281b1b" } else { "#1c1c1e" },
                                        if event.read_at.is_none() { "#ff453a" } else { "#2c2c2e" }
                                    ),
                                    div {
                                        style: "flex: 1;",
                                        div { style: "font-weight: 600; font-size: 0.8125rem; color: #fff;", "{event.device_name} — {event.metric_type.to_uppercase()}" }
                                        div { style: "font-size: 0.75rem; color: #ccc; margin-top: 0.15rem;", "{event.message}" }
                                        div { style: "font-size: 0.68rem; color: #777; margin-top: 0.25rem;", "{event.created_at}" }
                                    }
                                    if event.read_at.is_none() {
                                        button {
                                            style: "background: #333; border: 1px solid #444; color: #ccc; border-radius: 4px; padding: 0.2rem 0.5rem; font-size: 0.7rem; cursor: pointer; margin-left: 0.5rem;",
                                            onclick: {
                                                let id = event.id.clone();
                                                let t = props.token.clone();
                                                move |_| {
                                                    let id = id.clone();
                                                    let t = t.clone();
                                                    spawn(async move {
                                                        let client = reqwest::Client::new();
                                                        let _ = client.post(format!("https://backend-api.krequiem.workers.dev/api/alerts/history/{}/read", id)).bearer_auth(&t).send().await;
                                                        if let Ok(res) = client.get("https://backend-api.krequiem.workers.dev/api/alerts/history").bearer_auth(&t).send().await {
                                                            if let Ok(events) = res.json::<Vec<AlertEvent>>().await {
                                                                alert_history.set(events);
                                                            }
                                                        }
                                                    });
                                                }
                                            },
                                            "Mark Read"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Modal: Rules Overview
            if *is_rules_overview_open.read() {
                div {
                    style: "position: fixed; top: 0; left: 0; width: 100vw; height: 100vh; background-color: rgba(0,0,0,0.6); display: flex; align-items: center; justify-content: center; z-index: 1000;",
                    div {
                        style: "background-color: #28282a; padding: 1.75rem; border-radius: 10px; border: 1px solid #3a3a3c; width: 100%; max-width: 480px; max-height: 80vh; display: flex; flex-direction: column; gap: 1rem;",
                        div {
                            style: "display: flex; justify-content: space-between; align-items: center;",
                            h3 { style: "margin: 0; color: #fff; font-size: 1.05rem; font-weight: 600;", "Active Alert Rules" }
                            button {
                                style: "background: none; border: none; color: #8e8e93; cursor: pointer; padding: 0.2rem;",
                                onclick: move |_| is_rules_overview_open.set(false),
                                svg {
                                    style: "width: 14px; height: 14px; fill: currentColor;",
                                    view_box: "0 0 24 24",
                                    path { d: "M19 6.41L17.59 5 12 10.59 6.41 5 5 6.41 10.59 12 5 17.59 6.41 19 12 13.41 17.59 19 19 17.59 13.41 12z" }
                                }
                            }
                        }

                        div {
                            style: "flex: 1; overflow-y: auto; display: flex; flex-direction: column; gap: 0.4rem;",
                            if alert_rules.read().is_empty() {
                                div { style: "font-size: 0.8125rem; color: #666; font-style: italic; text-align: center; padding: 2rem;", "No alert rules configured. Click any statistic on the dashboard to set one!" }
                            }
                            for rule in alert_rules.read().iter() {
                                div {
                                    key: "{rule.id}",
                                    style: "background: #1c1c1e; border: 1px solid #2c2c2e; padding: 0.65rem 0.85rem; border-radius: 6px; display: flex; justify-content: space-between; align-items: center;",
                                    div {
                                        div { style: "font-weight: 600; font-size: 0.8125rem; color: #007aff;", "{rule.metric_type.to_uppercase()} > {rule.threshold_value}%" }
                                        div { style: "font-size: 0.72rem; color: #888; margin-top: 0.15rem;",
                                            "Target: "
                                            {
                                                if let Some(did) = &rule.device_id {
                                                    devices.read().iter().find(|d| &d.id == did).map(|d| d.name.clone()).unwrap_or_else(|| "Device".to_string())
                                                } else {
                                                    "All Devices".to_string()
                                                }
                                            }
                                        }
                                    }
                                    button {
                                        style: "background: none; border: 1px solid #444; color: #ff453a; border-radius: 4px; padding: 0.2rem 0.5rem; font-size: 0.7rem; cursor: pointer;",
                                        onclick: {
                                            let rid = rule.id.clone();
                                            let t = props.token.clone();
                                            move |_| {
                                                let rid = rid.clone();
                                                let t = t.clone();
                                                spawn(async move {
                                                    let client = reqwest::Client::new();
                                                    let url = format!("https://backend-api.krequiem.workers.dev/api/alerts/rules/{}", rid);
                                                    let _ = client.delete(&url).bearer_auth(&t).send().await;
                                                    if let Ok(res) = client.get("https://backend-api.krequiem.workers.dev/api/alerts/rules").bearer_auth(&t).send().await {
                                                        if let Ok(data) = res.json::<Vec<AlertRule>>().await {
                                                            alert_rules.set(data);
                                                        }
                                                    }
                                                });
                                            }
                                        },
                                        "Delete"
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
