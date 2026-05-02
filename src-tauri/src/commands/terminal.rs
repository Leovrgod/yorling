use serde::Serialize;
use std::collections::HashMap;
#[cfg(unix)]
use std::fs::File;
use std::io::{Read, Write};
#[cfg(unix)]
use std::os::fd::{AsRawFd, FromRawFd};
#[cfg(unix)]
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::process::Command;
#[cfg(not(unix))]
use std::process::{Child, ChildStdin, Stdio};
#[cfg(not(unix))]
use std::sync::Arc;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TerminalAgentCommand {
    provider_id: &'static str,
    display_name: &'static str,
    shell_command: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TerminalAppCommand {
    id: &'static str,
    display_name: &'static str,
    bundle_id: Option<&'static str>,
    app_paths: &'static [&'static str],
}

#[derive(Debug, Serialize)]
pub struct DetectedTerminalAgent {
    id: String,
    label: String,
    command: String,
    installed: bool,
    path: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct DetectedTerminalApp {
    id: String,
    label: String,
    installed: bool,
    embedded: bool,
    bundle_id: Option<String>,
    app_path: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct TerminalInventory {
    agents: Vec<DetectedTerminalAgent>,
    terminals: Vec<DetectedTerminalApp>,
}

#[derive(Debug, Serialize)]
pub struct EmbeddedTerminalLaunch {
    id: String,
    cwd: String,
    provider_id: String,
    display_name: String,
}

#[derive(Clone, Debug, Serialize)]
struct EmbeddedTerminalData {
    id: String,
    stream: String,
    data: String,
}

#[derive(Clone, Debug, Serialize)]
struct EmbeddedTerminalExit {
    id: String,
    code: Option<i32>,
}

#[cfg(unix)]
struct EmbeddedTerminalRecord {
    writer: File,
    pid: libc::pid_t,
}

#[cfg(not(unix))]
struct EmbeddedTerminalRecord {
    stdin: ChildStdin,
    child: Arc<Mutex<Child>>,
}

const TERMINAL_AGENT_COMMANDS: &[TerminalAgentCommand] = &[
    TerminalAgentCommand {
        provider_id: "codex",
        display_name: "Codex CLI",
        shell_command: "codex",
    },
    TerminalAgentCommand {
        provider_id: "claude-code",
        display_name: "Claude Code",
        shell_command: "claude",
    },
    TerminalAgentCommand {
        provider_id: "gemini",
        display_name: "Gemini CLI",
        shell_command: "gemini",
    },
    TerminalAgentCommand {
        provider_id: "cursor",
        display_name: "Cursor Agent",
        shell_command: "cursor-agent",
    },
    TerminalAgentCommand {
        provider_id: "copilot",
        display_name: "GitHub Copilot",
        shell_command: "copilot",
    },
    TerminalAgentCommand {
        provider_id: "opencode",
        display_name: "OpenCode",
        shell_command: "opencode",
    },
    TerminalAgentCommand {
        provider_id: "qwen-code",
        display_name: "Qwen Code",
        shell_command: "qwen",
    },
    TerminalAgentCommand {
        provider_id: "hermes",
        display_name: "Hermes",
        shell_command: "hermes",
    },
    TerminalAgentCommand {
        provider_id: "openclaw",
        display_name: "OpenClaw",
        shell_command: "openclaw",
    },
    TerminalAgentCommand {
        provider_id: "qoder",
        display_name: "Qoder",
        shell_command: "qoder",
    },
    TerminalAgentCommand {
        provider_id: "qoderwork",
        display_name: "QoderWork",
        shell_command: "qoderwork",
    },
    TerminalAgentCommand {
        provider_id: "codebuddy",
        display_name: "CodeBuddy",
        shell_command: "codebuddy",
    },
    TerminalAgentCommand {
        provider_id: "workbuddy",
        display_name: "WorkBuddy",
        shell_command: "workbuddy",
    },
];

#[cfg(target_os = "macos")]
const TERMINAL_APP_COMMANDS: &[TerminalAppCommand] = &[
    TerminalAppCommand {
        id: "terminal",
        display_name: "Terminal",
        bundle_id: Some("com.apple.Terminal"),
        app_paths: &[
            "/System/Applications/Utilities/Terminal.app",
            "/Applications/Utilities/Terminal.app",
            "/Applications/Terminal.app",
        ],
    },
    TerminalAppCommand {
        id: "ghostty",
        display_name: "Ghostty",
        bundle_id: Some("com.mitchellh.ghostty"),
        app_paths: &["/Applications/Ghostty.app"],
    },
    TerminalAppCommand {
        id: "iterm2",
        display_name: "iTerm2",
        bundle_id: Some("com.googlecode.iterm2"),
        app_paths: &["/Applications/iTerm.app", "/Applications/iTerm2.app"],
    },
    TerminalAppCommand {
        id: "wezterm",
        display_name: "WezTerm",
        bundle_id: Some("com.github.wez.wezterm"),
        app_paths: &["/Applications/WezTerm.app"],
    },
    TerminalAppCommand {
        id: "alacritty",
        display_name: "Alacritty",
        bundle_id: Some("org.alacritty"),
        app_paths: &["/Applications/Alacritty.app"],
    },
    TerminalAppCommand {
        id: "kitty",
        display_name: "kitty",
        bundle_id: Some("net.kovidgoyal.kitty"),
        app_paths: &["/Applications/kitty.app"],
    },
    TerminalAppCommand {
        id: "warp",
        display_name: "Warp",
        bundle_id: Some("dev.warp.Warp-Stable"),
        app_paths: &["/Applications/Warp.app"],
    },
];

#[cfg(not(target_os = "macos"))]
const TERMINAL_APP_COMMANDS: &[TerminalAppCommand] = &[];

static EMBEDDED_TERMINALS: OnceLock<Mutex<HashMap<String, EmbeddedTerminalRecord>>> =
    OnceLock::new();

fn embedded_terminals() -> &'static Mutex<HashMap<String, EmbeddedTerminalRecord>> {
    EMBEDDED_TERMINALS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn terminal_agent_command(provider_id: &str) -> Option<TerminalAgentCommand> {
    TERMINAL_AGENT_COMMANDS
        .iter()
        .copied()
        .find(|agent| agent.provider_id == provider_id)
}

fn terminal_app_command(terminal_id: &str) -> Option<TerminalAppCommand> {
    TERMINAL_APP_COMMANDS
        .iter()
        .copied()
        .find(|terminal| terminal.id == terminal_id)
}

fn expand_user_path(raw_path: &str) -> Result<PathBuf, String> {
    if raw_path == "~" {
        return dirs::home_dir().ok_or_else(|| "Cannot resolve the home directory".to_string());
    }

    if let Some(rest) = raw_path.strip_prefix("~/") {
        let home =
            dirs::home_dir().ok_or_else(|| "Cannot resolve the home directory".to_string())?;
        return Ok(home.join(rest));
    }

    Ok(PathBuf::from(raw_path))
}

fn resolve_project_dir(raw_path: &str) -> Result<PathBuf, String> {
    let path = expand_user_path(raw_path.trim())?;
    let metadata = std::fs::metadata(&path)
        .map_err(|error| format!("Cannot access {}: {}", path.display(), error))?;

    if !metadata.is_dir() {
        return Err(format!("{} is not a folder", path.display()));
    }

    path.canonicalize()
        .map_err(|error| format!("Cannot resolve {}: {}", path.display(), error))
}

fn shell_quote(value: &str) -> String {
    if value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '/' | '-' | '_' | '.' | ':'))
    {
        return value.to_string();
    }

    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn shell_quote_path(path: &Path) -> String {
    shell_quote(&path.to_string_lossy())
}

fn applescript_escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn common_path_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    if let Some(path_env) = std::env::var_os("PATH") {
        dirs.extend(std::env::split_paths(&path_env));
    }

    dirs.extend([
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/local/bin"),
        PathBuf::from("/usr/bin"),
        PathBuf::from("/bin"),
        PathBuf::from("/opt/local/bin"),
    ]);

    dirs
}

fn is_executable_file(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path)
            .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    }

    #[cfg(not(unix))]
    {
        true
    }
}

fn resolve_command_path(command: &str) -> Option<PathBuf> {
    if command.contains('/') || command.contains('\\') {
        let path = PathBuf::from(command);
        return is_executable_file(&path).then_some(path);
    }

    for dir in common_path_dirs() {
        let candidate = dir.join(command);
        if is_executable_file(&candidate) {
            return Some(candidate);
        }
    }

    None
}

fn resolve_command_with_shell(command: &str) -> Option<String> {
    resolve_command_path(command)
        .map(|path| path.to_string_lossy().to_string())
        .or_else(|| {
            let shell = if Path::new("/bin/zsh").exists() {
                "/bin/zsh"
            } else {
                "sh"
            };
            let output = Command::new(shell)
                .arg("-lc")
                .arg(format!("command -v {}", shell_quote(command)))
                .output()
                .ok()?;
            if !output.status.success() {
                return None;
            }

            let resolved = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if resolved.is_empty() {
                None
            } else {
                Some(resolved)
            }
        })
}

fn detected_app_path(terminal: TerminalAppCommand) -> Option<String> {
    terminal
        .app_paths
        .iter()
        .find(|path| Path::new(path).exists())
        .map(|path| (*path).to_string())
}

fn default_shell() -> String {
    std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string())
}

fn make_agent_shell_script(project_dir: &Path, agent: TerminalAgentCommand) -> String {
    format!(
        "cd {} && clear && {}",
        shell_quote_path(project_dir),
        agent.shell_command
    )
}

fn terminal_size(cols: Option<u16>, rows: Option<u16>) -> (u16, u16) {
    (
        cols.filter(|value| *value >= 2).unwrap_or(120),
        rows.filter(|value| *value >= 1).unwrap_or(32),
    )
}

fn create_embedded_terminal_id(provider_id: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    format!("embedded-terminal-{provider_id}-{nanos}")
}

fn read_embedded_stream<R>(app: AppHandle, id: String, stream: &'static str, mut reader: R)
where
    R: Read + Send + 'static,
{
    std::thread::spawn(move || {
        let mut buffer = [0_u8; 4096];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(count) => {
                    let data = String::from_utf8_lossy(&buffer[..count]).to_string();
                    let _ = app.emit(
                        "embedded-terminal-data",
                        EmbeddedTerminalData {
                            id: id.clone(),
                            stream: stream.to_string(),
                            data,
                        },
                    );
                }
                Err(_) => break,
            }
        }
    });
}

#[tauri::command]
pub fn get_terminal_inventory() -> TerminalInventory {
    let agents = TERMINAL_AGENT_COMMANDS
        .iter()
        .map(|agent| {
            let path = resolve_command_with_shell(agent.shell_command);
            DetectedTerminalAgent {
                id: agent.provider_id.to_string(),
                label: agent.display_name.to_string(),
                command: agent.shell_command.to_string(),
                installed: path.is_some(),
                path,
            }
        })
        .collect();

    let mut terminals = vec![DetectedTerminalApp {
        id: "embedded".to_string(),
        label: "Default".to_string(),
        installed: true,
        embedded: true,
        bundle_id: None,
        app_path: None,
    }];

    terminals.extend(TERMINAL_APP_COMMANDS.iter().map(|terminal| {
        let app_path = detected_app_path(*terminal);
        DetectedTerminalApp {
            id: terminal.id.to_string(),
            label: terminal.display_name.to_string(),
            installed: app_path.is_some(),
            embedded: false,
            bundle_id: terminal.bundle_id.map(str::to_string),
            app_path,
        }
    }));

    TerminalInventory { agents, terminals }
}

#[tauri::command]
pub fn launch_agent_terminal(
    cwd: String,
    provider_id: String,
    terminal_id: Option<String>,
) -> Result<(), String> {
    let project_dir = resolve_project_dir(&cwd)?;
    let agent = terminal_agent_command(&provider_id)
        .ok_or_else(|| format!("Unknown terminal agent: {provider_id}"))?;
    let terminal_id = terminal_id.unwrap_or_else(|| "terminal".to_string());

    if terminal_id == "embedded" {
        return Err("Use launch_embedded_agent_terminal for the embedded terminal".into());
    }

    launch_terminal_for_agent(&project_dir, agent, &terminal_id)
}

#[tauri::command]
pub fn launch_embedded_agent_terminal(
    app: AppHandle,
    cwd: String,
    provider_id: String,
    cols: Option<u16>,
    rows: Option<u16>,
) -> Result<EmbeddedTerminalLaunch, String> {
    let project_dir = resolve_project_dir(&cwd)?;
    let agent = terminal_agent_command(&provider_id)
        .ok_or_else(|| format!("Unknown terminal agent: {provider_id}"))?;

    let id = create_embedded_terminal_id(agent.provider_id);
    let shell_script = make_agent_shell_script(&project_dir, agent);
    let (cols, rows) = terminal_size(cols, rows);

    #[cfg(unix)]
    {
        let (reader, writer, pid) = spawn_embedded_pty(&project_dir, &shell_script, cols, rows)
            .map_err(|error| {
                format!(
                    "Failed to launch embedded terminal for {}: {}",
                    agent.display_name, error
                )
            })?;

        read_embedded_stream(app.clone(), id.clone(), "pty", reader);

        embedded_terminals()
            .lock()
            .map_err(|_| "Embedded terminal registry is unavailable".to_string())?
            .insert(id.clone(), EmbeddedTerminalRecord { writer, pid });

        wait_for_embedded_pty_exit(app.clone(), id.clone(), pid);

        return Ok(EmbeddedTerminalLaunch {
            id,
            cwd: project_dir.to_string_lossy().to_string(),
            provider_id: agent.provider_id.to_string(),
            display_name: agent.display_name.to_string(),
        });
    }

    #[cfg(not(unix))]
    {
        let mut child = Command::new(default_shell())
            .arg("-lc")
            .arg(shell_script)
            .current_dir(&project_dir)
            .env("TERM", "xterm-256color")
            .env("COLORTERM", "truecolor")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| {
                format!(
                    "Failed to launch embedded terminal for {}: {}",
                    agent.display_name, error
                )
            })?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| "Embedded terminal did not expose stdin".to_string())?;

        if let Some(stdout) = child.stdout.take() {
            read_embedded_stream(app.clone(), id.clone(), "stdout", stdout);
        }
        if let Some(stderr) = child.stderr.take() {
            read_embedded_stream(app.clone(), id.clone(), "stderr", stderr);
        }

        let child = Arc::new(Mutex::new(child));
        embedded_terminals()
            .lock()
            .map_err(|_| "Embedded terminal registry is unavailable".to_string())?
            .insert(
                id.clone(),
                EmbeddedTerminalRecord {
                    stdin,
                    child: Arc::clone(&child),
                },
            );

        let app_for_wait = app.clone();
        let id_for_wait = id.clone();
        std::thread::spawn(move || {
            let code = child
                .lock()
                .ok()
                .and_then(|mut child| child.wait().ok())
                .and_then(|status| status.code());

            if let Ok(mut records) = embedded_terminals().lock() {
                records.remove(&id_for_wait);
            }

            let _ = app_for_wait.emit(
                "embedded-terminal-exit",
                EmbeddedTerminalExit {
                    id: id_for_wait,
                    code,
                },
            );
        });

        Ok(EmbeddedTerminalLaunch {
            id,
            cwd: project_dir.to_string_lossy().to_string(),
            provider_id: agent.provider_id.to_string(),
            display_name: agent.display_name.to_string(),
        })
    }
}

#[tauri::command]
pub fn send_embedded_terminal_input(id: String, data: String) -> Result<(), String> {
    let mut records = embedded_terminals()
        .lock()
        .map_err(|_| "Embedded terminal registry is unavailable".to_string())?;
    let record = records
        .get_mut(&id)
        .ok_or_else(|| format!("Embedded terminal is not running: {id}"))?;

    #[cfg(unix)]
    {
        return record
            .writer
            .write_all(data.as_bytes())
            .and_then(|_| record.writer.flush())
            .map_err(|error| format!("Failed to write to embedded terminal: {error}"));
    }

    #[cfg(not(unix))]
    {
        record
            .stdin
            .write_all(data.as_bytes())
            .and_then(|_| record.stdin.flush())
            .map_err(|error| format!("Failed to write to embedded terminal: {error}"))
    }
}

#[tauri::command]
pub fn resize_embedded_terminal(id: String, cols: u16, rows: u16) -> Result<(), String> {
    let (cols, rows) = terminal_size(Some(cols), Some(rows));
    let records = embedded_terminals()
        .lock()
        .map_err(|_| "Embedded terminal registry is unavailable".to_string())?;
    let record = records
        .get(&id)
        .ok_or_else(|| format!("Embedded terminal is not running: {id}"))?;

    #[cfg(unix)]
    {
        let winsize = libc::winsize {
            ws_row: rows,
            ws_col: cols,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        let result = unsafe {
            libc::ioctl(
                record.writer.as_raw_fd(),
                libc::TIOCSWINSZ,
                &winsize as *const libc::winsize,
            )
        };
        if result == 0 {
            Ok(())
        } else {
            Err(format!(
                "Failed to resize embedded terminal: {}",
                std::io::Error::last_os_error()
            ))
        }
    }

    #[cfg(not(unix))]
    {
        let _ = record;
        Ok(())
    }
}

#[tauri::command]
pub fn stop_embedded_terminal(id: String) -> Result<(), String> {
    let record = embedded_terminals()
        .lock()
        .map_err(|_| "Embedded terminal registry is unavailable".to_string())?
        .remove(&id)
        .ok_or_else(|| format!("Embedded terminal is not running: {id}"))?;

    #[cfg(unix)]
    {
        let group_result = unsafe { libc::kill(-record.pid, libc::SIGTERM) };
        if group_result == 0 {
            return Ok(());
        }

        let process_result = unsafe { libc::kill(record.pid, libc::SIGTERM) };
        if process_result == 0 {
            Ok(())
        } else {
            Err(format!(
                "Failed to stop embedded terminal: {}",
                std::io::Error::last_os_error()
            ))
        }
    }

    #[cfg(not(unix))]
    {
        record
            .child
            .lock()
            .map_err(|_| "Embedded terminal process is unavailable".to_string())?
            .kill()
            .map_err(|error| format!("Failed to stop embedded terminal: {error}"))
    }
}

#[cfg(unix)]
fn spawn_embedded_pty(
    project_dir: &Path,
    shell_script: &str,
    cols: u16,
    rows: u16,
) -> std::io::Result<(File, File, libc::pid_t)> {
    let shell = default_shell();
    let shell_path = std::ffi::CString::new(shell.as_str()).map_err(invalid_input)?;
    let shell_name = Path::new(&shell)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("sh");
    let arg0 = std::ffi::CString::new(shell_name).map_err(invalid_input)?;
    let arg_lc = std::ffi::CString::new("-lc").map_err(invalid_input)?;
    let script = std::ffi::CString::new(shell_script).map_err(invalid_input)?;
    let cwd = std::ffi::CString::new(project_dir.as_os_str().as_bytes()).map_err(invalid_input)?;
    let term_key = std::ffi::CString::new("TERM").map_err(invalid_input)?;
    let term_value = std::ffi::CString::new("xterm-256color").map_err(invalid_input)?;
    let color_key = std::ffi::CString::new("COLORTERM").map_err(invalid_input)?;
    let color_value = std::ffi::CString::new("truecolor").map_err(invalid_input)?;

    let mut master_fd = -1;
    let mut winsize = libc::winsize {
        ws_row: rows,
        ws_col: cols,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };

    let pid = unsafe {
        libc::forkpty(
            &mut master_fd,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut winsize,
        )
    };

    if pid < 0 {
        return Err(std::io::Error::last_os_error());
    }

    if pid == 0 {
        unsafe {
            libc::chdir(cwd.as_ptr());
            libc::setenv(term_key.as_ptr(), term_value.as_ptr(), 1);
            libc::setenv(color_key.as_ptr(), color_value.as_ptr(), 1);
            libc::execl(
                shell_path.as_ptr(),
                arg0.as_ptr(),
                arg_lc.as_ptr(),
                script.as_ptr(),
                std::ptr::null::<libc::c_char>(),
            );
            libc::_exit(127);
        }
    }

    let writer = unsafe { File::from_raw_fd(master_fd) };
    let reader = writer.try_clone()?;
    Ok((reader, writer, pid))
}

#[cfg(unix)]
fn invalid_input(error: std::ffi::NulError) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidInput, error)
}

#[cfg(unix)]
fn wait_for_embedded_pty_exit(app: AppHandle, id: String, pid: libc::pid_t) {
    std::thread::spawn(move || {
        let mut status = 0;
        let waited = unsafe { libc::waitpid(pid, &mut status, 0) };
        let code = if waited > 0 {
            decode_wait_status(status)
        } else {
            None
        };

        if let Ok(mut records) = embedded_terminals().lock() {
            records.remove(&id);
        }

        let _ = app.emit("embedded-terminal-exit", EmbeddedTerminalExit { id, code });
    });
}

#[cfg(unix)]
fn decode_wait_status(status: libc::c_int) -> Option<i32> {
    if libc::WIFEXITED(status) {
        Some(libc::WEXITSTATUS(status))
    } else if libc::WIFSIGNALED(status) {
        Some(128 + libc::WTERMSIG(status))
    } else {
        None
    }
}

#[cfg(target_os = "macos")]
fn launch_terminal_for_agent(
    project_dir: &Path,
    agent: TerminalAgentCommand,
    terminal_id: &str,
) -> Result<(), String> {
    let shell_script = make_agent_shell_script(project_dir, agent);

    match terminal_id {
        "terminal" => launch_terminal_app(project_dir, agent, &shell_script),
        "iterm2" => launch_iterm2(agent, &shell_script),
        "ghostty" => launch_ghostty(project_dir, agent, &shell_script),
        "wezterm" => launch_app_binary(
            agent,
            &["/Applications/WezTerm.app/Contents/MacOS/wezterm"],
            vec![
                "start".to_string(),
                "--cwd".to_string(),
                project_dir.to_string_lossy().to_string(),
                default_shell(),
                "-lc".to_string(),
                shell_script.clone(),
            ],
        ),
        "alacritty" => launch_app_binary(
            agent,
            &["/Applications/Alacritty.app/Contents/MacOS/alacritty"],
            vec![
                "--working-directory".to_string(),
                project_dir.to_string_lossy().to_string(),
                "-e".to_string(),
                default_shell(),
                "-lc".to_string(),
                shell_script.clone(),
            ],
        ),
        "kitty" => launch_app_binary(
            agent,
            &["/Applications/kitty.app/Contents/MacOS/kitty"],
            vec![
                "--directory".to_string(),
                project_dir.to_string_lossy().to_string(),
                default_shell(),
                "-lc".to_string(),
                shell_script.clone(),
            ],
        ),
        "warp" => launch_warp(project_dir, agent, &shell_script),
        other => {
            if terminal_app_command(other).is_none() {
                return Err(format!("Unknown terminal app: {other}"));
            }
            launch_terminal_app(project_dir, agent, &shell_script)
        }
    }
}

#[cfg(target_os = "macos")]
fn launch_terminal_app(
    _project_dir: &Path,
    agent: TerminalAgentCommand,
    shell_script: &str,
) -> Result<(), String> {
    let script = format!(
        "tell application \"Terminal\"\n  activate\n  do script \"{}\"\nend tell",
        applescript_escape(shell_script)
    );

    let status = Command::new("osascript")
        .arg("-e")
        .arg(script)
        .status()
        .map_err(|error| {
            format!(
                "Failed to launch Terminal for {}: {}",
                agent.display_name, error
            )
        })?;

    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "Terminal launch failed for {} with status {}",
            agent.display_name, status
        ))
    }
}

#[cfg(target_os = "macos")]
fn launch_iterm2(agent: TerminalAgentCommand, shell_script: &str) -> Result<(), String> {
    let script = format!(
        "tell application \"iTerm2\"\n  activate\n  create window with default profile\n  tell current session of current window\n    write text \"{}\"\n  end tell\nend tell",
        applescript_escape(shell_script)
    );

    let status = Command::new("osascript")
        .arg("-e")
        .arg(script)
        .status()
        .map_err(|error| {
            format!(
                "Failed to launch iTerm2 for {}: {}",
                agent.display_name, error
            )
        })?;

    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "iTerm2 launch failed for {} with status {}",
            agent.display_name, status
        ))
    }
}

#[cfg(target_os = "macos")]
fn ghostty_open_arguments(project_dir: &Path, shell_script: &str) -> Vec<String> {
    vec![
        "-n".to_string(),
        "-b".to_string(),
        "com.mitchellh.ghostty".to_string(),
        "--args".to_string(),
        format!("--working-directory={}", project_dir.to_string_lossy()),
        "--window-inherit-working-directory=false".to_string(),
        format!("--input=raw:{shell_script}\n"),
    ]
}

#[cfg(target_os = "macos")]
fn launch_ghostty(
    project_dir: &Path,
    agent: TerminalAgentCommand,
    shell_script: &str,
) -> Result<(), String> {
    let args = ghostty_open_arguments(project_dir, shell_script);
    let status = Command::new("/usr/bin/open")
        .args(&args)
        .status()
        .map_err(|error| {
            format!(
                "Failed to launch Ghostty for {}: {}",
                agent.display_name, error
            )
        })?;

    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "Ghostty launch failed for {} with status {}",
            agent.display_name, status
        ))
    }
}

#[cfg(target_os = "macos")]
fn launch_app_binary(
    agent: TerminalAgentCommand,
    binary_paths: &[&str],
    args: Vec<String>,
) -> Result<(), String> {
    let binary = binary_paths
        .iter()
        .find(|path| Path::new(path).exists())
        .ok_or_else(|| format!("No supported launcher was found for {}", agent.display_name))?;

    Command::new(binary)
        .args(args)
        .spawn()
        .map(|_| ())
        .map_err(|error| {
            format!(
                "Failed to launch terminal for {}: {}",
                agent.display_name, error
            )
        })
}

#[cfg(target_os = "macos")]
fn launch_warp(
    project_dir: &Path,
    agent: TerminalAgentCommand,
    shell_script: &str,
) -> Result<(), String> {
    if resolve_command_path("warp").is_some() {
        return Command::new("warp")
            .arg("launch")
            .arg("--cwd")
            .arg(project_dir)
            .arg("--")
            .arg(default_shell())
            .arg("-lc")
            .arg(shell_script)
            .spawn()
            .map(|_| ())
            .map_err(|error| {
                format!(
                    "Failed to launch Warp for {}: {}",
                    agent.display_name, error
                )
            });
    }

    launch_terminal_app(project_dir, agent, shell_script)
}

#[cfg(target_os = "linux")]
fn launch_terminal_for_agent(
    project_dir: &Path,
    agent: TerminalAgentCommand,
    _terminal_id: &str,
) -> Result<(), String> {
    let shell_script = make_agent_shell_script(project_dir, agent);

    let candidates: &[(&str, &[&str])] = &[
        ("x-terminal-emulator", &["-e"]),
        ("gnome-terminal", &["--"]),
        ("konsole", &["-e"]),
        ("xfce4-terminal", &["-e"]),
        ("xterm", &["-e"]),
    ];

    for (terminal, prefix_args) in candidates {
        let mut command = Command::new(terminal);
        command
            .args(*prefix_args)
            .arg("sh")
            .arg("-lc")
            .arg(&shell_script);

        if command.spawn().is_ok() {
            return Ok(());
        }
    }

    Err(format!(
        "No supported terminal app was found for {}",
        agent.display_name
    ))
}

#[cfg(target_os = "windows")]
fn launch_terminal_for_agent(
    project_dir: &Path,
    agent: TerminalAgentCommand,
    _terminal_id: &str,
) -> Result<(), String> {
    let shell_script = format!(
        "cd /d {} && {}",
        shell_quote_path(project_dir),
        agent.shell_command
    );

    Command::new("cmd")
        .arg("/C")
        .arg("start")
        .arg("cmd")
        .arg("/K")
        .arg(shell_script)
        .spawn()
        .map(|_| ())
        .map_err(|error| {
            format!(
                "Failed to launch terminal for {}: {}",
                agent.display_name, error
            )
        })
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn launch_terminal_for_agent(
    _project_dir: &Path,
    agent: TerminalAgentCommand,
    _terminal_id: &str,
) -> Result<(), String> {
    Err(format!(
        "Launching {} is not supported on this platform yet",
        agent.display_name
    ))
}

#[cfg(test)]
mod tests {
    #[cfg(target_os = "macos")]
    use super::ghostty_open_arguments;
    use super::{get_terminal_inventory, shell_quote, terminal_agent_command};
    #[cfg(target_os = "macos")]
    use std::path::Path;

    #[test]
    fn shell_quote_keeps_safe_paths_plain() {
        assert_eq!(shell_quote("/Users/example/project-1"), "/Users/example/project-1");
    }

    #[test]
    fn shell_quote_escapes_single_quotes() {
        assert_eq!(
            shell_quote("/Users/example/what's next"),
            "'/Users/example/what'\"'\"'s next'"
        );
    }

    #[test]
    fn agent_command_maps_provider_ids_to_cli_commands() {
        assert_eq!(
            terminal_agent_command("claude-code").map(|agent| agent.shell_command),
            Some("claude")
        );
        assert_eq!(
            terminal_agent_command("qwen-code").map(|agent| agent.shell_command),
            Some("qwen")
        );
        assert!(terminal_agent_command("unknown").is_none());
    }

    #[test]
    fn inventory_always_exposes_embedded_terminal() {
        let inventory = get_terminal_inventory();
        assert!(
            inventory
                .terminals
                .iter()
                .any(|terminal| terminal.id == "embedded"
                    && terminal.installed
                    && terminal.embedded)
        );
        assert!(inventory.agents.iter().any(|agent| agent.id == "codex"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn ghostty_launcher_uses_launchservices_and_initial_input() {
        let args = ghostty_open_arguments(
            Path::new("/Users/example/project"),
            "cd /Users/example/project && codex",
        );

        assert_eq!(
            &args[..5],
            [
                "-n",
                "-b",
                "com.mitchellh.ghostty",
                "--args",
                "--working-directory=/Users/example/project",
            ]
        );
        assert!(!args.iter().any(|arg| arg == "--working-directory"));
        assert!(
            args.iter()
                .any(|arg| arg == "--window-inherit-working-directory=false")
        );
        assert!(
            args.iter()
                .any(|arg| arg == "--input=raw:cd /Users/example/project && codex\n")
        );
        assert!(!args.iter().any(|arg| arg == "-e"));
    }
}
