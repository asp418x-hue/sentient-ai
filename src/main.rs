// src/main.rs
mod cbt;

use axum::{routing::get, Router};
use cbt::{CbtEngine, CandidatePath};

#[tokio::main]
async fn main() {
    println!("Initializing Sentient-AI Core Node...");

    let engine = CbtEngine::new();

    // Example run of a cognitive loop cycle
    let exam = engine.examine("SYSTEM_EVENT: Low-level module calibration requested.");
    
    let candidates = vec![
        CandidatePath {
            id: "vector_a".to_string(),
            description: "Direct bare-metal syscall hook with minimal overhead".to_string(),
            resource_cost: 10,
            confidence_score: 0.91,
        },
        CandidatePath {
            id: "vector_b".to_string(),
            description: "High-level framework wrapper with heavy abstraction".to_string(),
            resource_cost: 75,
            confidence_score: 0.88,
        },
        CandidatePath {
            id: "vector_c".to_string(),
            description: "Speculative fallback routine with unverified state".to_string(),
            resource_cost: 12,
            confidence_score: 0.52, // Will fail confidence gate
        },
    ];

    match engine.process_of_elimination(candidates.clone()) {
        Ok(selected) => {
            let synopsis = engine.oem_synopsis(&exam, &candidates, &selected);
            println!("\n--- OEM SYNOPSIS LOG ---");
            println!("Observed State: {}", synopsis.observed_state);
            for (id, reason) in synopsis.eliminated_vectors {
                println!("Eliminated [{}]: {}", id, reason);
            }
            println!("Action: {}", synopsis.chosen_action);
            println!("Result: {}\n", synopsis.result_summary);
        }
        Err(e) => eprintln!("Cognitive cycle halted: {}", e),
    }

    // Initialize Axum server interface
    let app = Router::new().route("/health", get(|| async { "Sentient-AI Core Active" }));

    let listener = tokio::net::TcpListener::bind("127.0.0.1:8080").await.unwrap();
    println!("API Gateway bound to http://127.0.0.1:8080");
    axum::serve(listener, app).await.unwrap();
}

