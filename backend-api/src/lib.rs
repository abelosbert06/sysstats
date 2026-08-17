use shared_types::{BatchMetricRequest, SystemMetricPayload};
use worker::wasm_bindgen::{JsCast, JsValue};
use worker::*;
use uuid::Uuid;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct DeviceRecord {
    id: String,
    name: String,
    last_seen: String,
}

#[derive(Serialize, Deserialize)]
struct RegisterDeviceRequest {
    name: String,
}

async fn verify_user(req: &Request, env: &Env) -> Result<String> {
    let auth_header = req.headers().get("Authorization")?.unwrap_or_default();
    if !auth_header.starts_with("Bearer ") {
        return Err(Error::from("Missing or invalid Authorization header"));
    }
    
    // For local dev, if SUPABASE_URL isn't set, we mock a user ID for simplicity
    let supabase_url = match env.var("SUPABASE_URL") {
        Ok(v) => v.to_string(),
        Err(_) => return Ok("local-dev-user-id".to_string()),
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
    }
    
    let user: SupabaseUser = resp.json().await?;
    Ok(user.id)
}

#[event(fetch)]
pub async fn main(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    if req.method() == Method::Options {
        let mut resp = Response::empty()?;
        resp.headers_mut().set("Access-Control-Allow-Origin", "*")?;
        resp.headers_mut().set("Access-Control-Allow-Headers", "Authorization, Content-Type")?;
        resp.headers_mut().set("Access-Control-Allow-Methods", "GET, POST, PUT, DELETE, OPTIONS")?;
        return Ok(resp);
    }

    let router = Router::new()
        .post_async("/api/devices", |mut req, ctx| async move {
            let user_id = match verify_user(&req, &ctx.env).await {
                Ok(id) => id,
                Err(e) => return Response::error(e.to_string(), 401),
            };

            let req_data: RegisterDeviceRequest = match req.json().await {
                Ok(data) => data,
                Err(_) => return Response::error("Invalid JSON", 400),
            };

            let d1 = ctx.env.d1("DB")?;
            let device_id = Uuid::new_v4().to_string();
            let auth_token = Uuid::new_v4().to_string();

            // Insert into users table if not exists
            let placeholder_email = format!("{}@placeholder.com", user_id);
            let _ = d1.prepare("INSERT OR IGNORE INTO users (id, email) VALUES (?1, ?2)")
                .bind(&[user_id.clone().into(), placeholder_email.into()])?
                .run().await?;

            let res = d1.prepare("INSERT INTO devices (id, user_id, name, auth_token) VALUES (?1, ?2, ?3, ?4)")
                .bind(&[device_id.clone().into(), user_id.into(), req_data.name.into(), auth_token.clone().into()])?
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
            let user_id = match verify_user(&req, &ctx.env).await {
                Ok(id) => id,
                Err(e) => return Response::error(e.to_string(), 401),
            };

            let d1 = ctx.env.d1("DB")?;
            let res = d1.prepare("SELECT id, name, last_seen FROM devices WHERE user_id = ?1")
                .bind(&[user_id.into()])?
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
            let auth_res = d1.prepare("SELECT id FROM devices WHERE auth_token = ?1")
                .bind(&[auth_token.clone().into()])?
                .first::<String>(Some("id")).await?;

            let device_id = match auth_res {
                Some(id) => id,
                None => {
                    // For local dev, allow passing through without real D1 device if token is "dev_token"
                    if auth_token == "dev_token" {
                        "local-dev-device-id".to_string()
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
            }

            // Analytics Engine
            if let Ok(analytics_binding) = js_sys::Reflect::get(&ctx.env, &JsValue::from_str("TELEMETRY_ANALYTICS")) {
                if !analytics_binding.is_undefined() {
                    if let Ok(val) = js_sys::Reflect::get(&analytics_binding, &JsValue::from_str("writeDataPoint")) {
                        if let Ok(write_fn) = val.dyn_into::<js_sys::Function>() {
                            for metric in batch.metrics {
                                let dp = js_sys::Object::new();
                                let blobs = js_sys::Array::new();
                                blobs.push(&JsValue::from_str(&metric.device_id));

                                let doubles = js_sys::Array::new();
                                doubles.push(&JsValue::from_f64(metric.timestamp_sec as f64));
                                doubles.push(&JsValue::from_f64(metric.cpu_usage_pct as f64));
                                doubles.push(&JsValue::from_f64(metric.memory_used_mb as f64));
                                doubles.push(&JsValue::from_f64(metric.memory_total_mb as f64));
                                doubles.push(&JsValue::from_f64(metric.network_rx_bytes_sec as f64));
                                doubles.push(&JsValue::from_f64(metric.network_tx_bytes_sec as f64));
                                doubles.push(&JsValue::from_f64(metric.cpu_temperature_c as f64));
                                doubles.push(&JsValue::from_f64(metric.disk_usage_pct as f64));
                                doubles.push(&JsValue::from_f64(metric.uptime_sec as f64));
                                doubles.push(&JsValue::from_f64(metric.running_processes as f64));

                                let _ = js_sys::Reflect::set(&dp, &JsValue::from_str("blobs"), &blobs);
                                let _ = js_sys::Reflect::set(&dp, &JsValue::from_str("doubles"), &doubles);

                                let _ = write_fn.call1(&analytics_binding, &dp);
                            }
                        }
                    }
                }
            }

            Response::ok("Metrics ingested")
        })
        .get_async("/api/metrics/:device_id", |req, ctx| async move {
            let user_id = match verify_user(&req, &ctx.env).await {
                Ok(id) => id,
                Err(e) => return Response::error(e.to_string(), 401),
            };

            let device_id = match ctx.param("device_id") {
                Some(id) => id,
                None => return Response::error("Missing device ID", 400),
            };

            if user_id != "local-dev-user-id" {
                let d1 = ctx.env.d1("DB")?;
                let res = d1.prepare("SELECT id FROM devices WHERE id = ?1 AND user_id = ?2")
                    .bind(&[device_id.clone().into(), user_id.into()])?
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
            let user_id = match verify_user(&req, &ctx.env).await {
                Ok(id) => id,
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
                .bind(&[req_data.name.into(), device_id.clone().into(), user_id.into()])?
                .run().await;

            if res.is_err() {
                return Response::error("Failed to rename device", 500);
            }

            Response::ok("Device renamed")
        })
        .delete_async("/api/devices/:device_id", |req, ctx| async move {
            let user_id = match verify_user(&req, &ctx.env).await {
                Ok(id) => id,
                Err(e) => return Response::error(e.to_string(), 401),
            };

            let device_id = match ctx.param("device_id") {
                Some(id) => id,
                None => return Response::error("Missing device ID", 400),
            };

            let d1 = ctx.env.d1("DB")?;
            let res = d1.prepare("DELETE FROM devices WHERE id = ?1 AND user_id = ?2")
                .bind(&[device_id.clone().into(), user_id.into()])?
                .run().await;

            if res.is_err() {
                return Response::error("Failed to delete device", 500);
            }

            Response::ok("Device deleted")
        });
        
        let mut resp = router.run(req, env).await?;
        resp.headers_mut().set("Access-Control-Allow-Origin", "*")?;
        resp.headers_mut().set("Access-Control-Allow-Headers", "Authorization, Content-Type")?;
        resp.headers_mut().set("Access-Control-Allow-Methods", "GET, POST, PUT, DELETE, OPTIONS")?;
        Ok(resp)
}
