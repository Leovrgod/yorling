use clap::Parser;
use serde::Serialize;
use std::io::Read;
use std::path::Path;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "yorling-bridge", about = "Yorling Island Hook Bridge")]
struct Args {
    /// Source agent identifier (e.g. "claude", "codex", "gemini")
    #[arg(long)]
    source: String,

    /// Transport endpoint path for Unix domain sockets.
    #[cfg(unix)]
    #[arg(long)]
    socket: PathBuf,

    /// Transport endpoint path for Windows named pipes.
    #[cfg(windows)]
    #[arg(long)]
    pipe: PathBuf,

    /// Hook event type (e.g. "PreToolUse", "PostToolUse")
    #[arg(long)]
    event: String,
}

impl Args {
    fn transport_endpoint(&self) -> &Path {
        #[cfg(unix)]
        {
            &self.socket
        }

        #[cfg(windows)]
        {
            &self.pipe
        }
    }
}

/// Message sent to the island transport endpoint.
#[derive(Serialize)]
struct BridgePayload {
    hook_event: String,
    body: serde_json::Value,
    source: String,
    session_id: Option<String>,
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let args = Args::parse();
    let transport_endpoint = args.transport_endpoint().to_path_buf();

    // Read JSON from stdin
    let mut input = String::new();
    if let Err(e) = std::io::stdin().read_to_string(&mut input) {
        eprintln!("yorling-bridge: failed to read stdin: {}", e);
        std::process::exit(1);
    }

    let body: serde_json::Value = match serde_json::from_str(input.trim()) {
        Ok(v) => v,
        Err(_) => {
            // If stdin isn't valid JSON, wrap it
            serde_json::json!({ "raw": input.trim() })
        }
    };

    // Inject terminal context from environment variables so providers
    // can resolve the actual terminal/IDE for jump-to-terminal.
    let body = inject_terminal_context(body);

    // Extract session_id from the body if present
    let session_id = extract_session_id(&body);

    let payload = BridgePayload {
        hook_event: args.event,
        body,
        source: args.source,
        session_id,
    };

    let json_line = match serde_json::to_string(&payload) {
        Ok(j) => j,
        Err(e) => {
            eprintln!("yorling-bridge: failed to serialize: {}", e);
            std::process::exit(1);
        }
    };

    // Connect to the transport endpoint and send the message.
    match send_to_endpoint(&transport_endpoint, &json_line).await {
        Ok(response) => {
            if let Some(resp) = response {
                // Print response to stdout for blocking hooks
                println!("{}", resp);
            }
        }
        Err(e) => {
            // Silently fail — don't break the agent's workflow
            eprintln!("yorling-bridge: {}", e);
        }
    }
}

/// Inject terminal/IDE context from environment variables into the body JSON.
/// This allows providers to know which terminal app launched the agent,
/// enabling jump-to-terminal functionality.
fn inject_terminal_context(mut body: serde_json::Value) -> serde_json::Value {
    if let Some(obj) = body.as_object_mut() {
        // Inject TERM_PROGRAM (e.g., "Apple_Terminal", "iTerm.app", "WarpTerminal", "ghostty", "vscode")
        if !obj.contains_key("terminal_app") {
            if let Ok(term) = std::env::var("TERM_PROGRAM") {
                obj.insert("terminal_app".to_string(), serde_json::Value::String(term));
            }
        }

        // Inject __CFBundleIdentifier (e.g., "com.mitchellh.ghostty", "com.apple.Terminal")
        if !obj.contains_key("terminal_bundle_id") {
            if let Ok(bundle) = std::env::var("__CFBundleIdentifier") {
                obj.insert(
                    "terminal_bundle_id".to_string(),
                    serde_json::Value::String(bundle),
                );
            }
        }

        // Inject the parent PID so the island can reason about the actual
        // agent process instead of this short-lived bridge helper.
        if !obj.contains_key("pid") {
            if let Some(pid) = get_parent_pid() {
                obj.insert(
                    "pid".to_string(),
                    serde_json::Value::Number(serde_json::Number::from(pid)),
                );
            }
        }

        // Inject the bridge process PID as a fallback for terminal detection
        if !obj.contains_key("bridge_pid") {
            obj.insert(
                "bridge_pid".to_string(),
                serde_json::Value::Number(serde_json::Number::from(std::process::id())),
            );
        }

        // Inject the TTY device path (e.g., "/dev/ttys013") for precise terminal matching.
        // Each terminal tab/surface has a unique TTY, making this the most reliable
        // identifier when multiple windows of the same terminal app are open.
        if !obj.contains_key("tty") {
            if let Some(tty_path) = get_tty_path() {
                obj.insert("tty".to_string(), serde_json::Value::String(tty_path));
            }
        }
    }
    body
}

/// Get the TTY device path for the current agent session.
/// Prefers the parent process TTY, then falls back to stdio and the `tty` command.
fn get_tty_path() -> Option<String> {
    get_tty_path_from_parent_pid()
        .or_else(get_tty_path_from_stdio)
        .or_else(get_tty_path_from_command)
}

fn get_parent_pid() -> Option<u32> {
    #[cfg(unix)]
    unsafe {
        let pid = libc::getppid();
        if pid > 1 { Some(pid as u32) } else { None }
    }

    #[cfg(not(unix))]
    {
        None
    }
}

fn get_tty_path_from_parent_pid() -> Option<String> {
    let pid = get_parent_pid()?;
    let output = std::process::Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "tty="])
        .output()
        .ok()?;
    let tty = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if tty.is_empty() || tty == "??" || tty == "-" {
        None
    } else if tty.starts_with("/dev/") {
        Some(tty)
    } else {
        Some(format!("/dev/{tty}"))
    }
}

fn get_tty_path_from_stdio() -> Option<String> {
    // Try libc::ttyname on stdin (fd 0)
    #[cfg(unix)]
    {
        unsafe {
            let tty_ptr = libc::ttyname(libc::STDIN_FILENO);
            if !tty_ptr.is_null() {
                if let Ok(tty_str) = std::ffi::CStr::from_ptr(tty_ptr).to_str() {
                    if !tty_str.is_empty() {
                        return Some(tty_str.to_string());
                    }
                }
            }
        }
    }

    None
}

fn get_tty_path_from_command() -> Option<String> {
    std::process::Command::new("tty")
        .stdin(std::process::Stdio::inherit())
        .output()
        .ok()
        .and_then(|output| {
            let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if s.starts_with("/dev/") {
                Some(s)
            } else {
                None
            }
        })
}

async fn send_to_endpoint(
    endpoint_path: &Path,
    message: &str,
) -> Result<Option<String>, Box<dyn std::error::Error>> {
    #[cfg(unix)]
    {
        let stream = tokio::net::UnixStream::connect(endpoint_path).await?;
        return send_via_stream(stream, message).await;
    }

    #[cfg(windows)]
    {
        use tokio::net::windows::named_pipe::ClientOptions;

        const ERROR_PIPE_BUSY: i32 = 231;
        const PIPE_CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(1500);
        let pipe_name = endpoint_path.to_string_lossy().into_owned();
        let started_at = std::time::Instant::now();

        let client = loop {
            match ClientOptions::new().open(&pipe_name) {
                Ok(client) => break client,
                Err(error)
                    if (error.kind() == std::io::ErrorKind::NotFound
                        || error.raw_os_error() == Some(ERROR_PIPE_BUSY))
                        && started_at.elapsed() < PIPE_CONNECT_TIMEOUT => {}
                Err(error) => return Err(Box::new(error)),
            }

            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        };

        return send_via_stream(client, message).await;
    }
}

fn extract_session_id(body: &serde_json::Value) -> Option<String> {
    [
        "session_id",
        "sessionId",
        "conversation_id",
        "conversationId",
    ]
    .iter()
    .find_map(|key| body.get(*key).and_then(|value| value.as_str()))
    .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::extract_session_id;

    #[test]
    fn extract_session_id_supports_camel_case() {
        let body = serde_json::json!({
            "sessionId": "session-123"
        });

        assert_eq!(extract_session_id(&body).as_deref(), Some("session-123"));
    }

    #[test]
    fn extract_session_id_supports_conversation_id() {
        let body = serde_json::json!({
            "conversationId": "conversation-456"
        });

        assert_eq!(
            extract_session_id(&body).as_deref(),
            Some("conversation-456")
        );
    }
}

async fn send_via_stream<S>(
    stream: S,
    message: &str,
) -> Result<Option<String>, Box<dyn std::error::Error>>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, split};

    let (reader, mut writer) = split(stream);

    // Send message as a single line
    writer.write_all(message.as_bytes()).await?;
    writer.write_all(b"\n").await?;
    writer.flush().await?;
    // Signal we're done writing
    writer.shutdown().await?;

    // Read response (if any) with a timeout
    let mut buf_reader = BufReader::new(reader);
    let mut response = String::new();

    match tokio::time::timeout(
        std::time::Duration::from_secs(300),
        buf_reader.read_line(&mut response),
    )
    .await
    {
        Ok(Ok(0)) => Ok(None), // No response (non-blocking event)
        Ok(Ok(_)) => Ok(Some(response.trim().to_string())),
        Ok(Err(e)) => Err(Box::new(e)),
        Err(_) => Err("timeout waiting for response".into()),
    }
}
