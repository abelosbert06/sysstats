use shared_types::{AlertEvent, AlertRule, BatchMetricRequest, CreateAlertRuleRequest, SystemMetricPayload};
use worker::wasm_bindgen::{JsCast, JsValue};
use worker::*;
use uuid::Uuid;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone)]
struct DeviceRecord {
    id: String,
    name: String,
    last_seen: String,
}

#[derive(Serialize, Deserialize)]
struct DeviceAuthRecord {
    id: String,
    user_id: String,
    name: String,
}

#[derive(Serialize, Deserialize)]
struct RegisterDeviceRequest {
    name: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct DbAlertRule {
    id: String,
    user_id: String,
    device_id: Option<String>,
    metric_type: String,
    threshold_value: f64,
    cooldown_seconds: i64,
    notify_email: i64,
    notify_browser: i64,
    last_triggered: Option<String>,
    created_at: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct DbAlertHistory {
    id: String,
    user_id: String,
    device_id: String,
    device_name: String,
    metric_type: String,
    metric_value: f64,
    threshold_value: f64,
    message: String,
    created_at: String,
    read_at: Option<String>,
}

#[derive(Clone, Debug)]
struct VerifiedUser {
    id: String,
    email: Option<String>,
}

async fn verify_user(req: &Request, env: &Env) -> Result<VerifiedUser> {
    let auth_header = req.headers().get("Authorization")?.unwrap_or_default();
    if !auth_header.starts_with("Bearer ") {
        return Err(Error::from("Missing or invalid Authorization header"));
    }
    
    // For local dev, if SUPABASE_URL isn't set, we mock a user ID for simplicity
    let supabase_url = match env.var("SUPABASE_URL") {
        Ok(v) => v.to_string(),
        Err(_) => return Ok(VerifiedUser {
            id: "local-dev-user-id".to_string(),
            email: Some("dev@example.com".to_string()),
        }),
    };
    
    let mut headers = Headers::new();
    headers.set("Authorization", &auth_header)?;
    headers.set("apikey", &env.var("SUPABASE_ANON_KEY")?.to_string())?;

    let mut init = RequestInit::new();
    init.with_headers(headers);
    init.with_method(Method::Get);
    
    let url = format!("{}/auth/v1/user", supabase_url);
    let auth_req = Request::new_with_init(&url, &init)?;
    
    let mut resp = Fetch::Request(auth_req).send().await?;
    if resp.status_code() != 200 {
        return Err(Error::from("Invalid JWT token"));
    }
    
    #[derive(Deserialize)]
    struct SupabaseUser {
        id: String,
        email: Option<String>,
    }
    
    let user: SupabaseUser = resp.json().await?;
    Ok(VerifiedUser {
        id: user.id,
        email: user.email,
    })
}

async fn send_email_alert(env: &Env, to_email: &str, device_name: &str, metric: &str, current_val: f32, threshold: f32) {
    if let Ok(resend_key) = env.secret("RESEND_API_KEY") {
        let key_str = resend_key.to_string();
        if !key_str.is_empty() {
            let from_email = env.var("FROM_EMAIL").map(|v| v.to_string()).unwrap_or_else(|_| "alerts@sysstats.com".to_string());
            let subject = format!("[SysStats Alert] {} breached {} threshold on {}", metric.to_uppercase(), threshold, device_name);
            let html_body = format!(
                "<h2>SysStats Alert Notification</h2>\
                 <p><strong>Device:</strong> {}</p>\
                 <p><strong>Metric:</strong> {}</p>\
                 <p><strong>Current Value:</strong> {:.1}%</p>\
                 <p><strong>Configured Threshold:</strong> {:.1}%</p>\
                 <p>Log in to your dashboard to inspect active processes and telemetry.</p>",
                device_name, metric.to_uppercase(), current_val, threshold
            );

            let mut headers = Headers::new();
            let _ = headers.set("Authorization", &format!("Bearer {}", key_str));
            let _ = headers.set("Content-Type", "application/json");

            let mut init = RequestInit::new();
            init.with_headers(headers);
            init.with_method(Method::Post);
            init.with_body(Some(wasm_bindgen::JsValue::from_str(&serde_json::json!({
                "from": from_email,
                "to": [to_email],
                "subject": subject,
                "html": html_body
            }).to_string())));

            if let Ok(req) = Request::new_with_init("https://api.resend.com/emails", &init) {
                let _ = Fetch::Request(req).send().await;
            }
        }
    }
}

#[event(fetch)]
pub async fn main(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    if req.method() == Method::Options {
        let mut resp = Response::empty()?;
        resp.headers_mut().set("Access-Control-Allow-Origin", "*")?;
        resp.headers_mut().set("Access-Control-Allow-Headers", "*")?;
        resp.headers_mut().set("Access-Control-Allow-Methods", "GET, POST, PUT, DELETE, OPTIONS")?;
        resp.headers_mut().set("Access-Control-Max-Age", "86400")?;
        return Ok(resp);
    }

    let router = Router::new()
        .post_async("/api/devices", |mut req, ctx| async move {
            let user = match verify_user(&req, &ctx.env).await {
                Ok(u) => u,
                Err(e) => return Response::error(e.to_string(), 401),
            };

            let req_data: RegisterDeviceRequest = match req.json().await {
                Ok(data) => data,
                Err(_) => return Response::error("Invalid JSON", 400),
            };

            let d1 = ctx.env.d1("DB")?;
            let device_id = Uuid::new_v4().to_string();
            let auth_token = Uuid::new_v4().to_string();

            let email_to_store = user.email.unwrap_or_else(|| format!("{}@placeholder.com", user.id));
            let _ = d1.prepare("INSERT INTO users (id, email) VALUES (?1, ?2) ON CONFLICT(id) DO UPDATE SET email = ?2")
                .bind(&[user.id.clone().into(), email_to_store.clone().into()])?
                .run().await?;

            let res = d1.prepare("INSERT INTO devices (id, user_id, name, auth_token) VALUES (?1, ?2, ?3, ?4)")
                .bind(&[device_id.clone().into(), user.id.into(), req_data.name.into(), auth_token.clone().into()])?
                .run().await;

            if res.is_err() {
                return Response::error("Failed to register device", 500);
            }

            Response::from_json(&serde_json::json!({
                "device_id": device_id,
                "auth_token": auth_token
            }))
        })
        .get_async("/api/devices", |req, ctx| async move {
            let user = match verify_user(&req, &ctx.env).await {
                Ok(u) => u,
                Err(e) => return Response::error(e.to_string(), 401),
            };

            let d1 = ctx.env.d1("DB")?;
            let res = d1.prepare("SELECT id, name, last_seen FROM devices WHERE user_id = ?1")
                .bind(&[user.id.into()])?
                .all().await?;

            let devices = res.results::<DeviceRecord>()?;
            Response::from_json(&devices)
        })
        .post_async("/api/metrics", |mut req, ctx| async move {
            let auth_header = req.headers().get("Authorization")?.unwrap_or_default();
            let auth_token = auth_header.strip_prefix("Bearer ").unwrap_or("").to_string();
            
            if auth_token.is_empty() {
                return Response::error("Missing auth token", 401);
            }

            let d1 = ctx.env.d1("DB")?;
            let auth_res = d1.prepare("SELECT id, user_id, name FROM devices WHERE auth_token = ?1")
                .bind(&[auth_token.clone().into()])?
                .first::<DeviceAuthRecord>(None).await?;

            let (device_id, user_id, device_name) = match auth_res {
                Some(d) => (d.id, d.user_id, d.name),
                None => {
                    if auth_token == "dev_token" {
                        ("local-dev-device-id".to_string(), "local-dev-user-id".to_string(), "Dev Device".to_string())
                    } else {
                        return Response::error("Invalid device token", 401)
                    }
                }
            };

            let batch: BatchMetricRequest = match req.json().await {
                Ok(b) => b,
                Err(e) => return Response::error(format!("Invalid JSON: {}", e), 400),
            };

            if auth_token != "dev_token" {
                let _ = d1.prepare("UPDATE devices SET last_seen = CURRENT_TIMESTAMP WHERE id = ?1")
                    .bind(&[device_id.clone().into()])?
                    .run().await;
            }

            if let Some(latest) = batch.metrics.last() {
                let kv = ctx.env.kv("LIVE_METRICS")?;
                let json = serde_json::to_string(latest)?;
                let _ = kv.put(&device_id, json)?.execute().await;

                // Threshold Alerting Engine
                let rules_res = d1.prepare("SELECT id, user_id, device_id, metric_type, threshold_value, cooldown_seconds, notify_email, notify_browser, last_triggered, created_at FROM alert_rules WHERE user_id = ?1 AND (device_id = ?2 OR device_id IS NULL)")
                    .bind(&[user_id.clone().into(), device_id.clone().into()])?
                    .all().await;

                if let Ok(rules_query) = rules_res {
                    if let Ok(rules) = rules_query.results::<DbAlertRule>() {
                        for rule in rules {
                            let (metric_val, is_breached) = match rule.metric_type.as_str() {
                                "cpu" => (latest.cpu_usage_pct, latest.cpu_usage_pct >= rule.threshold_value as f32),
                                "memory" => {
                                    let mem_pct = if latest.memory_total_mb > 0 {
                                        (latest.memory_used_mb as f32 / latest.memory_total_mb as f32) * 100.0
                                    } else {
                                        0.0
                                    };
                                    (mem_pct, mem_pct >= rule.threshold_value as f32)
                                },
                                "disk" => (latest.disk_usage_pct, latest.disk_usage_pct >= rule.threshold_value as f32),
                                "temperature" => (latest.cpu_temperature_c, latest.cpu_temperature_c >= rule.threshold_value as f32),
                                _ => (0.0, false),
                            };

                            if is_breached {
                                // Check cooldown in SQL
                                let cooldown_res = d1.prepare("SELECT (last_triggered IS NULL OR (strftime('%s', 'now') - strftime('%s', last_triggered)) >= ?1) as ready FROM alert_rules WHERE id = ?2")
                                    .bind(&[((rule.cooldown_seconds as f64)).into(), rule.id.clone().into()])?
                                    .first::<i64>(Some("ready")).await;

                                if let Ok(Some(1)) = cooldown_res {
                                    let alert_id = Uuid::new_v4().to_string();
                                    let msg = format!(
                                        "{} breached threshold on {}: {:.1}% (Limit: {:.1}%)",
                                        rule.metric_type.to_uppercase(),
                                        device_name,
                                        metric_val,
                                        rule.threshold_value
                                    );

                                    // Record in alert_history
                                    let _ = d1.prepare("INSERT INTO alert_history (id, user_id, device_id, device_name, metric_type, metric_value, threshold_value, message) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)")
                                        .bind(&[
                                            alert_id.into(),
                                            user_id.clone().into(),
                                            device_id.clone().into(),
                                            device_name.clone().into(),
                                            rule.metric_type.clone().into(),
                                            (metric_val as f64).into(),
                                            rule.threshold_value.into(),
                                            msg.clone().into(),
                                        ])?
                                        .run().await;

                                    // Update last_triggered timestamp
                                    let _ = d1.prepare("UPDATE alert_rules SET last_triggered = CURRENT_TIMESTAMP WHERE id = ?1")
                                        .bind(&[rule.id.clone().into()])?
                                        .run().await;

                                    // Email dispatch if enabled
                                    if rule.notify_email == 1 {
                                        if let Ok(Some(user_email)) = d1.prepare("SELECT email FROM users WHERE id = ?1").bind(&[user_id.clone().into()])?.first::<String>(Some("email")).await {
                                            if !user_email.ends_with("@placeholder.com") {
                                                send_email_alert(&ctx.env, &user_email, &device_name, &rule.metric_type, metric_val, rule.threshold_value as f32).await;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            Response::ok("Metrics ingested")
        })
        .get_async("/api/metrics/:device_id", |req, ctx| async move {
            let user = match verify_user(&req, &ctx.env).await {
                Ok(u) => u,
                Err(e) => return Response::error(e.to_string(), 401),
            };

            let device_id = match ctx.param("device_id") {
                Some(id) => id,
                None => return Response::error("Missing device ID", 400),
            };

            if user.id != "local-dev-user-id" {
                let d1 = ctx.env.d1("DB")?;
                let res = d1.prepare("SELECT id FROM devices WHERE id = ?1 AND user_id = ?2")
                    .bind(&[device_id.clone().into(), user.id.into()])?
                    .first::<String>(Some("id")).await?;

                if res.is_none() {
                    return Response::error("Unauthorized access to device", 403);
                }
            }

            let kv = ctx.env.kv("LIVE_METRICS")?;
            let metric_json = kv.get(device_id).text().await?.unwrap_or_else(|| "null".to_string());
            
            let mut response = Response::ok(metric_json)?;
            response.headers_mut().set("Content-Type", "application/json")?;
            Ok(response)
        })
        .put_async("/api/devices/:device_id", |mut req, ctx| async move {
            let user = match verify_user(&req, &ctx.env).await {
                Ok(u) => u,
                Err(e) => return Response::error(e.to_string(), 401),
            };

            let device_id = match ctx.param("device_id") {
                Some(id) => id,
                None => return Response::error("Missing device ID", 400),
            };

            let req_data: RegisterDeviceRequest = match req.json().await {
                Ok(data) => data,
                Err(_) => return Response::error("Invalid JSON", 400),
            };

            let d1 = ctx.env.d1("DB")?;
            let res = d1.prepare("UPDATE devices SET name = ?1 WHERE id = ?2 AND user_id = ?3")
                .bind(&[req_data.name.into(), device_id.clone().into(), user.id.into()])?
                .run().await;

            if res.is_err() {
                return Response::error("Failed to rename device", 500);
            }

            Response::ok("Device renamed")
        })
        .delete_async("/api/devices/:device_id", |req, ctx| async move {
            let user = match verify_user(&req, &ctx.env).await {
                Ok(u) => u,
                Err(e) => return Response::error(e.to_string(), 401),
            };

            let device_id = match ctx.param("device_id") {
                Some(id) => id,
                None => return Response::error("Missing device ID", 400),
            };

            let d1 = ctx.env.d1("DB")?;
            let res = d1.prepare("DELETE FROM devices WHERE id = ?1 AND user_id = ?2")
                .bind(&[device_id.clone().into(), user.id.into()])?
                .run().await;

            if res.is_err() {
                return Response::error("Failed to delete device", 500);
            }

            Response::ok("Device deleted")
        })
        // Alert Rules Management
        .get_async("/api/alerts/rules", |req, ctx| async move {
            let user = match verify_user(&req, &ctx.env).await {
                Ok(u) => u,
                Err(e) => return Response::error(e.to_string(), 401),
            };

            let d1 = ctx.env.d1("DB")?;
            let res = d1.prepare("SELECT id, user_id, device_id, metric_type, threshold_value, cooldown_seconds, notify_email, notify_browser, last_triggered, created_at FROM alert_rules WHERE user_id = ?1 ORDER BY created_at DESC")
                .bind(&[user.id.into()])?
                .all().await?;

            let db_rules = res.results::<DbAlertRule>()?;
            let rules: Vec<AlertRule> = db_rules.into_iter().map(|r| AlertRule {
                id: r.id,
                user_id: r.user_id,
                device_id: r.device_id,
                metric_type: r.metric_type,
                threshold_value: r.threshold_value as f32,
                cooldown_seconds: r.cooldown_seconds as u32,
                notify_email: r.notify_email == 1,
                notify_browser: r.notify_browser == 1,
                last_triggered: r.last_triggered,
                created_at: r.created_at,
            }).collect();

            Response::from_json(&rules)
        })
        .post_async("/api/alerts/rules", |mut req, ctx| async move {
            let user = match verify_user(&req, &ctx.env).await {
                Ok(u) => u,
                Err(e) => return Response::error(e.to_string(), 401),
            };

            let req_data: CreateAlertRuleRequest = match req.json().await {
                Ok(d) => d,
                Err(_) => return Response::error("Invalid JSON", 400),
            };

            let d1 = ctx.env.d1("DB")?;
            let rule_id = Uuid::new_v4().to_string();
            let cooldown = req_data.cooldown_seconds.unwrap_or(900) as f64;
            let notify_email: f64 = if req_data.notify_email.unwrap_or(true) { 1.0 } else { 0.0 };
            let notify_browser: f64 = if req_data.notify_browser.unwrap_or(true) { 1.0 } else { 0.0 };

            let email_to_store = user.email.clone().unwrap_or_else(|| format!("{}@placeholder.com", user.id));
            let _ = d1.prepare("INSERT INTO users (id, email) VALUES (?1, ?2) ON CONFLICT(id) DO UPDATE SET email = ?2")
                .bind(&[user.id.clone().into(), email_to_store.clone().into()])?
                .run().await;

            let res = if let Some(ref did) = req_data.device_id {
                if !did.is_empty() {
                    d1.prepare("INSERT INTO alert_rules (id, user_id, device_id, metric_type, threshold_value, cooldown_seconds, notify_email, notify_browser) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)")
                        .bind(&[
                            rule_id.clone().into(),
                            user.id.into(),
                            did.clone().into(),
                            req_data.metric_type.into(),
                            (req_data.threshold_value as f64).into(),
                            cooldown.into(),
                            notify_email.into(),
                            notify_browser.into(),
                        ])?
                        .run().await
                } else {
                    d1.prepare("INSERT INTO alert_rules (id, user_id, device_id, metric_type, threshold_value, cooldown_seconds, notify_email, notify_browser) VALUES (?1, ?2, NULL, ?3, ?4, ?5, ?6, ?7)")
                        .bind(&[
                            rule_id.clone().into(),
                            user.id.into(),
                            req_data.metric_type.into(),
                            (req_data.threshold_value as f64).into(),
                            cooldown.into(),
                            notify_email.into(),
                            notify_browser.into(),
                        ])?
                        .run().await
                }
            } else {
                d1.prepare("INSERT INTO alert_rules (id, user_id, device_id, metric_type, threshold_value, cooldown_seconds, notify_email, notify_browser) VALUES (?1, ?2, NULL, ?3, ?4, ?5, ?6, ?7)")
                    .bind(&[
                        rule_id.clone().into(),
                        user.id.into(),
                        req_data.metric_type.into(),
                        (req_data.threshold_value as f64).into(),
                        cooldown.into(),
                        notify_email.into(),
                        notify_browser.into(),
                    ])?
                    .run().await
            };

            if let Err(e) = res {
                return Response::error(format!("Failed to create alert rule: {}", e), 500);
            }

            Response::from_json(&serde_json::json!({ "id": rule_id }))
        })
        .delete_async("/api/alerts/rules/:rule_id", |req, ctx| async move {
            let user = match verify_user(&req, &ctx.env).await {
                Ok(u) => u,
                Err(e) => return Response::error(e.to_string(), 401),
            };

            let rule_id = match ctx.param("rule_id") {
                Some(id) => id,
                None => return Response::error("Missing rule ID", 400),
            };

            let d1 = ctx.env.d1("DB")?;
            let res = d1.prepare("DELETE FROM alert_rules WHERE id = ?1 AND user_id = ?2")
                .bind(&[rule_id.clone().into(), user.id.into()])?
                .run().await;

            if res.is_err() {
                return Response::error("Failed to delete alert rule", 500);
            }

            Response::ok("Rule deleted")
        })
        // Alert History (Notifications Feed)
        .get_async("/api/alerts/history", |req, ctx| async move {
            let user = match verify_user(&req, &ctx.env).await {
                Ok(u) => u,
                Err(e) => return Response::error(e.to_string(), 401),
            };

            let d1 = ctx.env.d1("DB")?;
            let res = d1.prepare("SELECT id, user_id, device_id, device_name, metric_type, metric_value, threshold_value, message, created_at, read_at FROM alert_history WHERE user_id = ?1 ORDER BY created_at DESC LIMIT 30")
                .bind(&[user.id.into()])?
                .all().await?;

            let db_history = res.results::<DbAlertHistory>()?;
            let history: Vec<AlertEvent> = db_history.into_iter().map(|h| AlertEvent {
                id: h.id,
                user_id: h.user_id,
                device_id: h.device_id,
                device_name: h.device_name,
                metric_type: h.metric_type,
                metric_value: h.metric_value as f32,
                threshold_value: h.threshold_value as f32,
                message: h.message,
                created_at: h.created_at,
                read_at: h.read_at,
            }).collect();

            Response::from_json(&history)
        })
        .post_async("/api/alerts/history/:id/read", |req, ctx| async move {
            let user = match verify_user(&req, &ctx.env).await {
                Ok(u) => u,
                Err(e) => return Response::error(e.to_string(), 401),
            };

            let alert_id = match ctx.param("id") {
                Some(id) => id,
                None => return Response::error("Missing alert ID", 400),
            };

            let d1 = ctx.env.d1("DB")?;
            let _ = d1.prepare("UPDATE alert_history SET read_at = CURRENT_TIMESTAMP WHERE id = ?1 AND user_id = ?2")
                .bind(&[alert_id.clone().into(), user.id.into()])?
                .run().await;

            Response::ok("Marked as read")
        });
        
    let res = router.run(req, env).await;
    let mut resp = match res {
        Ok(r) => r,
        Err(e) => Response::error(format!("Internal Error: {}", e), 500)?,
    };
    resp.headers_mut().set("Access-Control-Allow-Origin", "*")?;
    resp.headers_mut().set("Access-Control-Allow-Headers", "*")?;
    resp.headers_mut().set("Access-Control-Allow-Methods", "GET, POST, PUT, DELETE, OPTIONS")?;
    Ok(resp)
}
