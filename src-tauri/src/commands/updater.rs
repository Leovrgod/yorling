use serde::Serialize;
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, State};
use tauri_plugin_updater::{Update, UpdaterExt};

const UPDATER_ENDPOINT: Option<&str> = option_env!("YORLING_UPDATER_ENDPOINT");
const UPDATER_PUBKEY: Option<&str> = option_env!("YORLING_UPDATER_PUBKEY");
const UPDATER_TARGET: Option<&str> = option_env!("YORLING_UPDATER_TARGET");

#[derive(Default)]
pub struct AppUpdaterState {
    pending_update: Mutex<Option<Update>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppUpdateCheckResult {
    configured: bool,
    available: bool,
    current_version: String,
    version: Option<String>,
    date: Option<String>,
    notes: Option<String>,
    message: Option<String>,
}

#[tauri::command]
pub async fn check_app_update(
    app: AppHandle,
    state: State<'_, AppUpdaterState>,
) -> Result<AppUpdateCheckResult, String> {
    let current_version = app.package_info().version.to_string();
    let Some(endpoint) = configured_value(UPDATER_ENDPOINT) else {
        clear_pending_update(&state);
        return Ok(AppUpdateCheckResult::not_configured(current_version));
    };
    let Some(pubkey) = configured_value(UPDATER_PUBKEY) else {
        clear_pending_update(&state);
        return Ok(AppUpdateCheckResult::not_configured(current_version));
    };

    let endpoint = endpoint
        .parse::<url::Url>()
        .map_err(|error| format!("Invalid updater endpoint: {error}"))?;

    let mut builder = app
        .updater_builder()
        .pubkey(pubkey)
        .timeout(Duration::from_secs(30))
        .endpoints(vec![endpoint])
        .map_err(|error| format!("Failed to configure updater endpoint: {error}"))?;

    if let Some(target) = configured_value(UPDATER_TARGET) {
        builder = builder.target(target);
    }

    let update = builder
        .build()
        .map_err(|error| format!("Failed to build updater: {error}"))?
        .check()
        .await
        .map_err(|error| format!("Failed to check for updates: {error}"))?;

    let result = match update.as_ref() {
        Some(update) => AppUpdateCheckResult {
            configured: true,
            available: true,
            current_version: update.current_version.clone(),
            version: Some(update.version.clone()),
            date: update.date.map(|date| date.to_string()),
            notes: update.body.clone(),
            message: None,
        },
        None => AppUpdateCheckResult {
            configured: true,
            available: false,
            current_version,
            version: None,
            date: None,
            notes: None,
            message: None,
        },
    };

    *state.pending_update.lock().unwrap() = update;
    Ok(result)
}

#[tauri::command]
pub async fn install_app_update(
    app: AppHandle,
    state: State<'_, AppUpdaterState>,
) -> Result<(), String> {
    let Some(update) = state.pending_update.lock().unwrap().take() else {
        return Err("No pending update. Check for updates first.".into());
    };

    let mut downloaded = 0usize;
    let install_result = update
        .download_and_install(
            |chunk_length, content_length| {
                downloaded += chunk_length;
                if let Some(content_length) = content_length {
                    log::debug!(
                        "Downloading Yorling update: {} / {} bytes",
                        downloaded,
                        content_length
                    );
                } else {
                    log::debug!("Downloading Yorling update: {} bytes", downloaded);
                }
            },
            || {
                log::info!("Yorling update package downloaded");
            },
        )
        .await;

    if let Err(error) = install_result {
        *state.pending_update.lock().unwrap() = Some(update);
        return Err(format!("Failed to install update: {error}"));
    }

    app.restart();
}

fn clear_pending_update(state: &State<'_, AppUpdaterState>) {
    *state.pending_update.lock().unwrap() = None;
}

fn configured_value(value: Option<&'static str>) -> Option<&'static str> {
    value.and_then(|value| {
        let value = value.trim();
        if value.is_empty() { None } else { Some(value) }
    })
}

impl AppUpdateCheckResult {
    fn not_configured(current_version: String) -> Self {
        Self {
            configured: false,
            available: false,
            current_version,
            version: None,
            date: None,
            notes: None,
            message: Some("Update service is not configured for this build.".into()),
        }
    }
}
