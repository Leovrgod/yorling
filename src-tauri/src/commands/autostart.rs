#[cfg(any(target_os = "windows", test))]
use std::path::Path;

pub const WINDOWS_AUTOSTART_ARG: &str = "--yorling-autostart";

#[tauri::command]
pub async fn is_windows_autostart_enabled(app: tauri::AppHandle) -> Result<bool, String> {
    #[cfg(target_os = "windows")]
    {
        windows::is_enabled(&autostart_app_name(&app))
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = app;
        Err("Windows autostart registry is only available on Windows.".into())
    }
}

#[tauri::command]
pub async fn set_windows_autostart_enabled(
    app: tauri::AppHandle,
    enabled: bool,
) -> Result<bool, String> {
    #[cfg(target_os = "windows")]
    {
        let app_name = autostart_app_name(&app);
        windows::set_enabled(&app_name, enabled)?;
        windows::is_enabled(&app_name)
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = (app, enabled);
        Err("Windows autostart registry is only available on Windows.".into())
    }
}

pub fn launched_from_windows_autostart() -> bool {
    std::env::args_os().any(|arg| arg == WINDOWS_AUTOSTART_ARG)
}

#[cfg(target_os = "windows")]
fn autostart_app_name(app: &tauri::AppHandle) -> String {
    app.package_info().name.to_string()
}

#[cfg(any(target_os = "windows", test))]
fn format_windows_run_command(exe_path: &Path, args: &[&str]) -> String {
    let mut command = quote_windows_command_arg(&exe_path.display().to_string());

    for arg in args {
        command.push(' ');
        command.push_str(&quote_windows_command_arg(arg));
    }

    command
}

#[cfg(any(target_os = "windows", test))]
fn quote_windows_command_arg(value: &str) -> String {
    let mut quoted = String::with_capacity(value.len() + 2);
    quoted.push('"');

    let mut pending_backslashes = 0usize;
    for character in value.chars() {
        match character {
            '\\' => pending_backslashes += 1,
            '"' => {
                for _ in 0..(pending_backslashes * 2 + 1) {
                    quoted.push('\\');
                }
                quoted.push('"');
                pending_backslashes = 0;
            }
            _ => {
                for _ in 0..pending_backslashes {
                    quoted.push('\\');
                }
                pending_backslashes = 0;
                quoted.push(character);
            }
        }
    }

    for _ in 0..(pending_backslashes * 2) {
        quoted.push('\\');
    }
    quoted.push('"');
    quoted
}

#[cfg(target_os = "windows")]
mod windows {
    use super::{WINDOWS_AUTOSTART_ARG, format_windows_run_command};
    use std::env::current_exe;
    use std::io::ErrorKind;
    use winreg::enums::RegType::REG_BINARY;
    use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_SET_VALUE};
    use winreg::{RegKey, RegValue};

    const RUN_KEY: &str = "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Run";
    const STARTUP_APPROVED_RUN_KEY: &str =
        "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Explorer\\StartupApproved\\Run";
    const STARTUP_APPROVED_ENABLED_VALUE: [u8; 12] = [
        0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];

    pub fn is_enabled(app_name: &str) -> Result<bool, String> {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let run_key = match hkcu.open_subkey_with_flags(RUN_KEY, KEY_READ) {
            Ok(key) => key,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(format!("Failed to open Windows Run registry key: {error}")),
        };

        let registered_command = match run_key.get_value::<String, _>(app_name) {
            Ok(command) => command,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(false),
            Err(error) => {
                return Err(format!(
                    "Failed to read Yorling autostart registry value: {error}"
                ));
            }
        };

        let expected_command = current_run_command()?;
        Ok(registered_command == expected_command && task_manager_allows_startup(app_name))
    }

    pub fn set_enabled(app_name: &str, enabled: bool) -> Result<(), String> {
        if enabled {
            enable(app_name)
        } else {
            disable(app_name)
        }
    }

    fn enable(app_name: &str) -> Result<(), String> {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let (run_key, _) = hkcu
            .create_subkey(RUN_KEY)
            .map_err(|error| format!("Failed to create Windows Run registry key: {error}"))?;
        run_key
            .set_value(app_name, &current_run_command()?)
            .map_err(|error| {
                format!("Failed to write Yorling autostart registry value: {error}")
            })?;

        if let Ok((startup_approved_key, _)) = hkcu.create_subkey(STARTUP_APPROVED_RUN_KEY) {
            let _ = startup_approved_key.set_raw_value(
                app_name,
                &RegValue {
                    vtype: REG_BINARY,
                    bytes: STARTUP_APPROVED_ENABLED_VALUE.to_vec(),
                },
            );
        }

        Ok(())
    }

    fn disable(app_name: &str) -> Result<(), String> {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        if let Ok(run_key) = hkcu.open_subkey_with_flags(RUN_KEY, KEY_SET_VALUE) {
            match run_key.delete_value(app_name) {
                Ok(()) => {}
                Err(error) if error.kind() == ErrorKind::NotFound => {}
                Err(error) => {
                    return Err(format!(
                        "Failed to remove Yorling autostart registry value: {error}"
                    ));
                }
            }
        }

        if let Ok(startup_approved_key) =
            hkcu.open_subkey_with_flags(STARTUP_APPROVED_RUN_KEY, KEY_SET_VALUE)
        {
            let _ = startup_approved_key.delete_value(app_name);
        }

        Ok(())
    }

    fn current_run_command() -> Result<String, String> {
        let exe_path = current_exe()
            .map_err(|error| format!("Failed to resolve current executable path: {error}"))?;
        Ok(format_windows_run_command(
            &exe_path,
            &[WINDOWS_AUTOSTART_ARG],
        ))
    }

    fn task_manager_allows_startup(app_name: &str) -> bool {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let startup_approved_key =
            match hkcu.open_subkey_with_flags(STARTUP_APPROVED_RUN_KEY, KEY_READ) {
                Ok(key) => key,
                Err(_) => return true,
            };
        let raw = match startup_approved_key.get_raw_value(app_name) {
            Ok(value) => value,
            Err(_) => return true,
        };

        raw.bytes.len() >= 8 && raw.bytes.iter().rev().take(8).all(|value| *value == 0)
    }
}

#[cfg(test)]
mod tests {
    use super::{WINDOWS_AUTOSTART_ARG, format_windows_run_command};
    use std::path::Path;

    #[test]
    fn windows_run_command_quotes_exe_path_and_keeps_startup_arg() {
        assert_eq!(
            format_windows_run_command(
                Path::new(r"C:\Program Files\Yorling\Yorling.exe"),
                &[WINDOWS_AUTOSTART_ARG],
            ),
            r#""C:\Program Files\Yorling\Yorling.exe" "--yorling-autostart""#,
        );
    }

    #[test]
    fn windows_run_command_escapes_trailing_backslashes() {
        assert_eq!(
            format_windows_run_command(Path::new(r"C:\Yorling\"), &[]),
            r#""C:\Yorling\\""#,
        );
    }
}
