use dioxus::prelude::*;
use super::AuthState;
use serde_json::json;

const SUPABASE_URL: &str = "https://mdeubpsdmmuowntstjjv.supabase.co";
const SUPABASE_ANON_KEY: &str = "sb_publishable_teBG-z74PbFwZKkgsOyrUw_gE8nVyWB";

#[component]
pub fn LoginScreen(mut auth_state: Signal<AuthState>) -> Element {
    let mut email = use_signal(|| String::new());
    let mut password = use_signal(|| String::new());
    let mut is_signup = use_signal(|| false);
    let mut error_msg = use_signal(|| String::new());
    let mut is_loading = use_signal(|| false);

    let handle_auth = move |_| {
        let e = email.read().clone();
        let p = password.read().clone();
        let signup = *is_signup.read();

        if e.is_empty() || p.is_empty() {
            error_msg.set("Email and password are required.".to_string());
            return;
        }

        is_loading.set(true);
        error_msg.set(String::new());

        spawn(async move {
            let client = reqwest::Client::new();
            
            let endpoint = if signup {
                format!("{}/auth/v1/signup", SUPABASE_URL)
            } else {
                format!("{}/auth/v1/token?grant_type=password", SUPABASE_URL)
            };

            let res = client.post(&endpoint)
                .header("apikey", SUPABASE_ANON_KEY)
                .json(&json!({
                    "email": e,
                    "password": p
                }))
                .send()
                .await;

            is_loading.set(false);

            match res {
                Ok(response) => {
                    if response.status().is_success() {
                        if let Ok(data) = response.json::<serde_json::Value>().await {
                            if let Some(token) = data.get("access_token").and_then(|v| v.as_str()) {
                                auth_state.set(AuthState::Authenticated { token: token.to_string() });
                            } else if signup {
                                error_msg.set("Registration successful! Please check your email to verify (if enabled).".to_string());
                            } else {
                                error_msg.set("Failed to retrieve access token.".to_string());
                            }
                        }
                    } else {
                        if let Ok(err_data) = response.json::<serde_json::Value>().await {
                            let msg = err_data.get("error_description")
                                .or_else(|| err_data.get("msg"))
                                .or_else(|| err_data.get("message"))
                                .and_then(|v| v.as_str())
                                .unwrap_or("Authentication failed.");
                            error_msg.set(msg.to_string());
                        } else {
                            error_msg.set("Authentication failed.".to_string());
                        }
                    }
                }
                Err(e) => {
                    error_msg.set(format!("Network error: {}", e));
                }
            }
        });
    };

    let loading = *is_loading.read();
    let btn_opacity = if loading { "0.7" } else { "1.0" };

    rsx! {
        div {
            style: "flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: center; background-color: #1e1e1e;",
            div {
                style: "background-color: #252525; padding: 3rem; border-radius: 12px; border: 1px solid #333; display: flex; flex-direction: column; gap: 1.5rem; width: 100%; max-width: 400px; box-shadow: 0 10px 30px rgba(0,0,0,0.5);",
                
                div {
                    style: "text-align: center;",
                    h2 { style: "margin: 0; color: #fff; font-size: 1.5rem;", "Activity Monitor" }
                    p { 
                        style: "color: #888; font-size: 0.875rem; margin-top: 0.5rem;", 
                        if *is_signup.read() { "Create a new account" } else { "Sign in to your account" } 
                    }
                }

                div {
                    style: "display: flex; flex-direction: column; gap: 1rem;",
                    
                    div {
                        style: "display: flex; flex-direction: column; gap: 0.25rem;",
                        label { style: "color: #aaa; font-size: 0.75rem; font-weight: 600; text-transform: uppercase; letter-spacing: 0.5px;", "Email" }
                        input {
                            type: "email",
                            placeholder: "you@example.com",
                            value: "{email}",
                            oninput: move |evt| email.set(evt.value()),
                            style: "background-color: #1e1e1e; border: 1px solid #444; color: #fff; padding: 0.75rem; border-radius: 6px; font-size: 1rem; outline: none; transition: border-color 0.2s;",
                        }
                    }

                    div {
                        style: "display: flex; flex-direction: column; gap: 0.25rem;",
                        label { style: "color: #aaa; font-size: 0.75rem; font-weight: 600; text-transform: uppercase; letter-spacing: 0.5px;", "Password" }
                        input {
                            type: "password",
                            placeholder: "••••••••",
                            value: "{password}",
                            oninput: move |evt| password.set(evt.value()),
                            style: "background-color: #1e1e1e; border: 1px solid #444; color: #fff; padding: 0.75rem; border-radius: 6px; font-size: 1rem; outline: none; transition: border-color 0.2s;",
                        }
                    }
                }

                if !error_msg.read().is_empty() {
                    div {
                        style: "color: #ff4444; font-size: 0.875rem; text-align: center; background-color: rgba(255,68,68,0.1); padding: 0.5rem; border-radius: 4px;",
                        "{error_msg}"
                    }
                }

                button {
                    style: "background-color: #007aff; color: #fff; border: none; padding: 0.75rem; border-radius: 6px; font-size: 1rem; font-weight: 600; cursor: pointer; margin-top: 0.5rem; opacity: {btn_opacity};",
                    disabled: *is_loading.read(),
                    onclick: handle_auth,
                    if *is_loading.read() {
                        "Loading..."
                    } else if *is_signup.read() {
                        "Sign Up"
                    } else {
                        "Sign In"
                    }
                }
                
                div {
                    style: "text-align: center; margin-top: 1rem;",
                    button { 
                        style: "background: none; border: none; color: #00bfff; font-size: 0.875rem; cursor: pointer; padding: 0;",
                        onclick: move |_| {
                            let current = *is_signup.read();
                            is_signup.set(!current);
                            error_msg.set(String::new());
                        },
                        if *is_signup.read() {
                            "Already have an account? Sign in."
                        } else {
                            "Need an account? Sign up."
                        }
                    }
                }
            }
        }
    }
}
