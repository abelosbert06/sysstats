pub mod auth;
pub mod dashboard;

use dioxus::prelude::*;

#[derive(Clone, Debug, PartialEq)]
pub enum AuthState {
    Unauthenticated,
    Authenticated { token: String },
}

#[component]
pub fn App() -> Element {
    let mut auth_state = use_signal(|| AuthState::Unauthenticated);

    rsx! {
        div {
            style: "height: 100vh; width: 100vw; background-color: #000000; color: #e0e0e0; font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Helvetica, Arial, sans-serif; display: flex; flex-direction: column; overflow: hidden;",
            style {
                "* {{ box-sizing: border-box; }}"
                "body {{ background-color: #000000; margin: 0; padding: 0; }}"
            }
            if let AuthState::Authenticated { token } = &*auth_state.read() {
                dashboard::Dashboard { 
                    token: token.clone(),
                    on_logout: move |_| {
                        #[cfg(not(target_arch = "wasm32"))]
                        {
                            let _ = crate::local_db::desktop::delete_device_token();
                            crate::collector::desktop::set_device_token(String::new());
                        }
                        auth_state.set(AuthState::Unauthenticated);
                    }
                }
            } else {
                auth::LoginScreen { auth_state }
            }
        }
    }
}
