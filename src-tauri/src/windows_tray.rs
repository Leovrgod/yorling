#![cfg(target_os = "windows")]

use crate::commands::keyboard::InterceptorState;
use std::sync::Arc;
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter};

const TRAY_ID: &str = "yorling-keyboard-tray";
const QUIT_MENU_ID: &str = "yorling-tray-quit";
const TRAY_EVENT: &str = "keyboard-tray-status-changed";

pub fn setup(app: &AppHandle, interceptor_state: Arc<InterceptorState>) -> tauri::Result<()> {
    let quit_item = MenuItem::with_id(app, QUIT_MENU_ID, "退出 Yorling", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&quit_item])?;
    let initial_active = interceptor_state.windows_keyboard_mapping_active();

    let state_for_click = Arc::clone(&interceptor_state);
    let state_for_menu = Arc::clone(&interceptor_state);

    TrayIconBuilder::with_id(TRAY_ID)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .icon(tray_image(initial_active)?)
        .tooltip(tray_tooltip(initial_active))
        .on_tray_icon_event(move |tray, event| {
            if !is_left_single_click(event) {
                return;
            }

            let active = match state_for_click.toggle_windows_keyboard_mapping() {
                Ok(active) => active,
                Err(error) => {
                    log::warn!("Failed to toggle Windows keyboard mapping from tray: {error}");
                    state_for_click.windows_keyboard_mapping_active()
                }
            };

            if let Err(error) = update_tray(tray, active) {
                log::warn!("Failed to refresh Windows tray icon: {error}");
            }

            if let Err(error) = tray.app_handle().emit(TRAY_EVENT, active) {
                log::warn!("Failed to emit Windows tray status change: {error}");
            }
        })
        .on_menu_event(move |app, event| {
            if event.id() == QUIT_MENU_ID {
                state_for_menu.shutdown_windows_keyboard_mapping();
                app.exit(0);
            }
        })
        .build(app)?;

    Ok(())
}

pub fn refresh(app: &AppHandle, active: bool) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };

    if let Err(error) = update_tray(&tray, active) {
        log::warn!("Failed to refresh Windows tray icon: {error}");
    }

    if let Err(error) = app.emit(TRAY_EVENT, active) {
        log::warn!("Failed to emit Windows tray status change: {error}");
    }
}

fn is_left_single_click(event: TrayIconEvent) -> bool {
    matches!(
        event,
        TrayIconEvent::Click {
            button: MouseButton::Left,
            button_state: MouseButtonState::Down,
            ..
        }
    )
}

fn update_tray(tray: &TrayIcon, active: bool) -> tauri::Result<()> {
    tray.set_icon(Some(tray_image(active)?))?;
    tray.set_tooltip(Some(tray_tooltip(active)))?;
    Ok(())
}

fn tray_tooltip(active: bool) -> &'static str {
    if active {
        "Yorling 键盘映射已开启，单击关闭"
    } else {
        "Yorling 键盘映射已关闭，单击开启"
    }
}

fn tray_image(active: bool) -> tauri::Result<Image<'static>> {
    let icon = Image::from_bytes(include_bytes!("../icons/32x32.png"))?.to_owned();
    if active {
        return Ok(icon);
    }

    Ok(dim_icon(icon))
}

fn dim_icon(icon: Image<'static>) -> Image<'static> {
    let width = icon.width();
    let height = icon.height();
    let mut rgba = icon.rgba().to_vec();

    for pixel in rgba.chunks_exact_mut(4) {
        pixel[0] = ((pixel[0] as f32) * 0.35) as u8;
        pixel[1] = ((pixel[1] as f32) * 0.35) as u8;
        pixel[2] = ((pixel[2] as f32) * 0.35) as u8;
        pixel[3] = ((pixel[3] as f32) * 0.82) as u8;
    }

    Image::new_owned(rgba, width, height)
}
