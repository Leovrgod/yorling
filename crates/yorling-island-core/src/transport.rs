use std::path::{Path, PathBuf};

use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader, split};
use tokio::sync::mpsc;
use tracing::{error, info, warn};

/// A message received from a bridge connection, with a reply channel for blocking responses.
pub struct BridgeMessage {
    pub data: Vec<u8>,
    /// Send a response back to the bridge (for blocking requests like PermissionRequest).
    /// If None, the bridge didn't expect a response.
    pub reply_tx: Option<mpsc::Sender<Vec<u8>>>,
}

/// Bridge transport endpoint.
///
/// This uses Unix domain sockets on macOS/Linux and Windows named pipes on Windows.
pub struct BridgeTransport {
    endpoint_path: PathBuf,
    message_tx: mpsc::Sender<BridgeMessage>,
}

impl BridgeTransport {
    pub fn new(endpoint_path: PathBuf) -> (Self, mpsc::Receiver<BridgeMessage>) {
        let (tx, rx) = mpsc::channel(128);
        (
            Self {
                endpoint_path,
                message_tx: tx,
            },
            rx,
        )
    }

    pub fn endpoint_path(&self) -> &Path {
        &self.endpoint_path
    }

    /// Start listening for connections. Runs until cancelled.
    #[cfg(unix)]
    pub async fn start(&self) -> anyhow::Result<()> {
        // Remove stale socket file
        let _ = tokio::fs::remove_file(&self.endpoint_path).await;

        // Ensure parent directory exists
        if let Some(parent) = self.endpoint_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let listener = tokio::net::UnixListener::bind(&self.endpoint_path)?;
        info!("Island socket listening at {:?}", self.endpoint_path);

        loop {
            match listener.accept().await {
                Ok((stream, _addr)) => {
                    let tx = self.message_tx.clone();
                    tokio::spawn(async move {
                        Self::handle_connection(stream, tx).await;
                    });
                }
                Err(e) => {
                    error!("Failed to accept connection: {}", e);
                }
            }
        }
    }

    /// Start listening for named pipe connections. Runs until cancelled.
    #[cfg(windows)]
    pub async fn start(&self) -> anyhow::Result<()> {
        use tokio::net::windows::named_pipe::ServerOptions;

        let pipe_name = self.endpoint_path.to_string_lossy().into_owned();
        let mut server = ServerOptions::new().create(&pipe_name)?;
        info!("Island named pipe listening at {}", pipe_name);

        loop {
            server.connect().await?;

            let tx = self.message_tx.clone();
            let connection = server;

            // Keep another server instance open so clients can connect continuously.
            server = ServerOptions::new().create(&pipe_name)?;

            tokio::spawn(async move {
                Self::handle_connection(connection, tx).await;
            });
        }
    }

    async fn handle_connection<T>(stream: T, message_tx: mpsc::Sender<BridgeMessage>)
    where
        T: AsyncRead + AsyncWrite + Unpin,
    {
        let (reader, mut writer) = split(stream);
        let mut buf_reader = BufReader::new(reader);
        let mut line = String::new();

        // Read one line of JSON from the bridge
        match buf_reader.read_line(&mut line).await {
            Ok(0) => return, // EOF
            Ok(_) => {}
            Err(e) => {
                warn!("Failed to read from bridge: {}", e);
                return;
            }
        }

        let line = line.trim().to_string();
        if line.is_empty() {
            return;
        }

        // Create a reply channel for blocking responses
        let (reply_tx, mut reply_rx) = mpsc::channel::<Vec<u8>>(1);

        let msg = BridgeMessage {
            data: line.into_bytes(),
            reply_tx: Some(reply_tx),
        };

        if message_tx.send(msg).await.is_err() {
            warn!("Message receiver dropped");
            return;
        }

        // Wait for a reply (with timeout for non-blocking events)
        match tokio::time::timeout(std::time::Duration::from_secs(300), reply_rx.recv()).await {
            Ok(Some(response)) => {
                let mut response_with_newline: Vec<u8> = response;
                response_with_newline.push(b'\n');
                if let Err(e) = writer.write_all(&response_with_newline).await {
                    warn!("Failed to write response to bridge: {}", e);
                }
            }
            Ok(None) => {
                // Reply channel closed without response — non-blocking event, just close
            }
            Err(_) => {
                warn!("Bridge response timed out after 300s");
            }
        }
    }

    /// Cleanup: remove the socket file.
    #[cfg(unix)]
    pub async fn cleanup(&self) {
        let _ = tokio::fs::remove_file(&self.endpoint_path).await;
    }

    /// Named pipes are ephemeral and do not need filesystem cleanup.
    #[cfg(windows)]
    pub async fn cleanup(&self) {}
}
