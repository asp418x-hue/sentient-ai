// src/main.rs
use sentient_ai::api::{self, ApiState};
use std::sync::Arc;

#[tokio::main]
async fn main() {
    println!("Initializing Sentient-AI Core Node...");

    // The native system is optional for development runs. Set
    // COTHINK_SYSTEM_DIR to enable the shared-library bridge.
    let native = if std::env::var_os("COTHINK_SYSTEM_DIR").is_some() {
        match sentient_ai::cothink_bridge::init_bridge() {
            Ok(native) => match native.initialize() {
                Ok(()) => {
                    println!("Native cothink dispatch ready");
                    Some(Arc::new(native))
                }
                Err(error) => {
                    eprintln!("Native cothink initialization failed: {error}");
                    None
                }
            },
            Err(error) => {
                eprintln!("Could not load native cothink library: {error}");
                None
            }
        }
    } else {
        None
    };

    let app = api::router(ApiState { native });

    let listener = tokio::net::TcpListener::bind("127.0.0.1:8080")
        .await
        .unwrap();
    println!("API Gateway bound to http://127.0.0.1:8080");
    axum::serve(listener, app).await.unwrap();
}
