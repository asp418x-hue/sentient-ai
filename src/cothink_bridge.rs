use std::process::Stdio;
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Command, Child};
use tokio::sync::mpsc;
use std::error::Error;

pub struct CothinkBridge {
    child_process: Child,
    stdin_sender: mpsc::Sender<String>,
}

impl CothinkBridge {
    /// Spawns the cothink-system binary and establishes async I/O channels
    pub async fn init(binary_path: &str) -> Result<Self, Box<dyn Error>> {
        let mut child = Command::new(binary_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        let stdout = child.stdout.take().ok_or("Failed to capture cothink-system stdout")?;
        let stderr = child.stderr.take().ok_or("Failed to capture cothink-system stderr")?;
        let stdin = child.stdin.take().ok_or("Failed to capture cothink-system stdin")?;

        let (tx, mut rx) = mpsc::channel::<String>(100);

        // Handle writing to cothink-system stdin asynchronously
        tokio::spawn(async move {
            let mut writer = stdin;
            while let Some(msg) = rx.recv().await {
                if let Err(e) = writer.write_all(msg.as_bytes()).await {
                    eprintln!("Failed to write to cothink-system stdin: {}", e);
                    break;
                }
                let _ = writer.flush().await;
            }
        });

        // Handle reading stdout from cothink-system
        tokio::spawn(async move {
            let reader = BufReader::new(stdout);
            let mut lines = tokio::io::AsyncBufReadExt::lines(reader);
            while let Ok(Some(line)) = lines.next_line().await {
                println!("[cothink-system stdout]: {}", line);
            }
        });

        // Handle reading stderr from cothink-system
        tokio::spawn(async move {
            let reader = BufReader::new(stderr);
            let mut lines = tokio::io::AsyncBufReadExt::lines(reader);
            while let Ok(Some(line)) = lines.next_line().await {
                eprintln!("[cothink-system stderr]: {}", line);
            }
        });

        Ok(Self {
            child_process: child,
            stdin_sender: tx,
        })
    }

    /// Send a query, telemetry packet, or CBT phase log directly into cothink-system
    pub async fn send_payload(&self, payload: &str) -> Result<(), Box<dyn Error>> {
        let formatted = format!("{}\n", payload);
        self.stdin_sender.send(formatted).await?;
        Ok(())
    }

    /// Gracefully shutdown the binary process
    pub async fn terminate(&mut self) -> Result<(), Box<dyn Error>> {
        self.child_process.kill().await?;
        Ok(())
    }
}

