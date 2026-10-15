use std::error::Error;
use std::path::PathBuf;
use tokio::net::{UnixListener, UnixStream};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub async fn run_am_socket_server() -> Result<(), Box<dyn Error>> {
    // Resolve home directory and construct ~/am.sock path
    let home_dir = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let socket_path = PathBuf::from(home_dir).join("am.sock");

    // Remove any stale socket file from prior executions
    if socket_path.exists() {
        tokio::fs::remove_file(&socket_path).await?;
        println!("Removed stale socket at: {:?}", socket_path);
    }

    // Bind the Tokio Unix listener
    let listener = UnixListener::bind(&socket_path)?;
    println!("AM Stream Listener active on: {:?}", socket_path);

    // Accept incoming streaming connections loop
    loop {
        match listener.accept().await {
            Ok((stream, _addr)) => {
                // Spawn a concurrent task for each connected client stream
                tokio::spawn(async move {
                    if let Err(e) = handle_stream(stream).await {
                        eprintln!("Stream processing error: {}", e);
                    }
                });
            }
            Err(e) => {
                eprintln!("Connection accept failed: {}", e);
            }
        }
    }
}

async fn handle_stream(mut stream: UnixStream) -> Result<(), Box<dyn Error>> {
    let (mut reader, mut writer) = stream.split();
    let mut buffer = vec![0u8; 1024];

    loop {
        let n = reader.read(&mut buffer).await?;
        if n == 0 {
            // Client closed connection
            break;
        }

        // Process or echo incoming streaming data bytes
        let received_data = &buffer[..n];
        println!("Received stream chunk: {} bytes", n);

        // Example: Echoing back acknowledgement or data stream payload
        writer.write_all(received_data).await?;
    }

    Ok(())
}

