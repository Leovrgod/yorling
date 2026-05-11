mod alt_tab;
mod bracket_overlay;
mod commands;
mod island;
#[cfg(target_os = "windows")]
mod windows_keyboard;
#[cfg(target_os = "windows")]
mod windows_single_instance;
#[cfg(target_os = "windows")]
mod windows_tray;
#[cfg(target_os = "windows")]
mod windows_window;

use commands::autostart::launched_from_windows_autostart;
use commands::clipboard::ClipboardState;
use commands::keyboard::InterceptorState;
use commands::super_right_click::SuperRightClickState;
use commands::updater::AppUpdaterState;
use island::IslandState;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tauri::{Manager, Runtime, WebviewWindow};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut};

#[cfg(target_os = "macos")]
use objc2_app_kit::NSWindow;

const MAIN_WINDOW_LABEL: &str = "main";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct MainWindowPresentationPlan {
    show: bool,
    unminimize: bool,
    focus: bool,
    order_front: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MainWindowLifecycle {
    Ready,
    Reopen { has_visible_windows: bool },
}

const fn main_window_presentation_plan(
    lifecycle: MainWindowLifecycle,
) -> MainWindowPresentationPlan {
    match lifecycle {
        MainWindowLifecycle::Ready => MainWindowPresentationPlan {
            show: true,
            unminimize: true,
            focus: true,
            order_front: true,
        },
        MainWindowLifecycle::Reopen {
            has_visible_windows,
        } => MainWindowPresentationPlan {
            show: !has_visible_windows,
            unminimize: true,
            focus: true,
            order_front: true,
        },
    }
}

pub(crate) fn present_main_window<R: Runtime>(
    app_handle: &tauri::AppHandle<R>,
    lifecycle: MainWindowLifecycle,
) {
    let Some(window) = app_handle.get_webview_window(MAIN_WINDOW_LABEL) else {
        log::warn!("Main window is unavailable during {lifecycle:?}");
        return;
    };

    present_window(&window, main_window_presentation_plan(lifecycle));
}

fn present_window<R: Runtime>(window: &WebviewWindow<R>, plan: MainWindowPresentationPlan) {
    if plan.show {
        if let Err(error) = window.show() {
            log::warn!("Failed to show the main window: {error}");
        }
    }

    if plan.unminimize {
        if let Err(error) = window.unminimize() {
            log::warn!("Failed to unminimize the main window: {error}");
        }
    }

    if plan.focus {
        if let Err(error) = window.set_focus() {
            log::warn!("Failed to focus the main window: {error}");
        }
    }

    #[cfg(target_os = "macos")]
    if plan.order_front {
        order_window_front(window);
    }
}

#[cfg(target_os = "macos")]
fn order_window_front<R: Runtime>(window: &WebviewWindow<R>) {
    let window_handle = window.clone();
    let window_for_lookup = window.clone();
    if let Err(error) = window_handle.run_on_main_thread(move || {
        let Some(ns_window) = get_ns_window(&window_for_lookup) else {
            return;
        };

        ns_window.orderFrontRegardless();
    }) {
        log::warn!("Failed to order the main window to the front: {error}");
    }
}

#[cfg(target_os = "macos")]
fn get_ns_window<R: Runtime>(window: &WebviewWindow<R>) -> Option<&NSWindow> {
    let ns_window_ptr = match window.ns_window() {
        Ok(handle) => handle,
        Err(error) => {
            log::warn!("Failed to access the native main window: {error}");
            return None;
        }
    };

    unsafe { (ns_window_ptr as *mut NSWindow).as_ref() }
}

fn register_island_shortcuts<R: Runtime>(app: &tauri::AppHandle<R>) {
    use tauri::Emitter;

    #[cfg(target_os = "macos")]
    let toggle_shortcut = Shortcut::new(Some(Modifiers::SUPER | Modifiers::SHIFT), Code::KeyI);
    #[cfg(not(target_os = "macos"))]
    let toggle_shortcut = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyI);

    #[cfg(target_os = "macos")]
    let approve_shortcut = Shortcut::new(Some(Modifiers::SUPER | Modifiers::SHIFT), Code::KeyY);
    #[cfg(not(target_os = "macos"))]
    let approve_shortcut = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyY);

    #[cfg(target_os = "macos")]
    let deny_shortcut = Shortcut::new(Some(Modifiers::SUPER | Modifiers::SHIFT), Code::KeyN);
    #[cfg(not(target_os = "macos"))]
    let deny_shortcut = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyN);

    let app_handle = app.clone();

    if let Err(e) = app.global_shortcut().on_shortcuts(
        [toggle_shortcut, approve_shortcut, deny_shortcut],
        move |_app, shortcut, _event| {
            if shortcut == &toggle_shortcut {
                let _ = app_handle.emit("island-shortcut", "toggle");
            } else if shortcut == &approve_shortcut {
                let _ = app_handle.emit("island-shortcut", "approve");
            } else if shortcut == &deny_shortcut {
                let _ = app_handle.emit("island-shortcut", "deny");
            }
        },
    ) {
        log::warn!("Failed to register island global shortcuts: {}", e);
    }
}

const fn should_start_island_runtime() -> bool {
    cfg!(any(target_os = "macos", target_os = "windows"))
}

pub fn run() {
    env_logger::init();

    #[cfg(target_os = "windows")]
    if !windows_single_instance::claim_or_focus_existing(!launched_from_windows_autostart()) {
        return;
    }

    let interceptor_state = Arc::new(InterceptorState::new());
    let island_state = Arc::new(IslandState::new());
    let clipboard_state = Arc::new(ClipboardState::new());
    let super_right_click_state = Arc::new(SuperRightClickState::new());
    let updater_state = AppUpdaterState::default();
    let suppress_ready_presentation_for_finder_action = Arc::new(AtomicBool::new(false));
    let suppress_ready_presentation_for_setup =
        suppress_ready_presentation_for_finder_action.clone();
    let suppress_ready_presentation_for_run = suppress_ready_presentation_for_finder_action.clone();

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .manage(interceptor_state)
        .manage(island_state.clone())
        .manage(clipboard_state.clone())
        .manage(super_right_click_state)
        .manage(updater_state)
        .setup(move |app| {
            #[cfg(any(target_os = "macos", windows, target_os = "linux"))]
            if let Err(error) = app
                .handle()
                .plugin(tauri_plugin_updater::Builder::new().build())
            {
                log::warn!("Failed to initialize updater plugin: {error}");
            }

            #[cfg(any(target_os = "macos", windows, target_os = "linux"))]
            if let Err(error) = app.handle().plugin(tauri_plugin_autostart::init(
                tauri_plugin_autostart::MacosLauncher::LaunchAgent,
                None,
            )) {
                log::warn!("Failed to initialize autostart plugin: {error}");
            }

            let state = app.state::<Arc<InterceptorState>>();
            state.bind_app_handle(app.handle().clone());

            #[cfg(target_os = "windows")]
            if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
                windows_window::configure_main_window(&window);
            }

            #[cfg(target_os = "windows")]
            if let Err(error) = windows_tray::setup(app.handle(), state.inner().clone()) {
                log::warn!("Failed to initialize Windows tray controls: {error}");
            }

            if should_start_island_runtime() {
                // Create island window (non-async, safe in setup)
                let island_state = app.state::<Arc<IslandState>>();
                let island_enabled = island_state
                    .enabled
                    .load(std::sync::atomic::Ordering::Relaxed);
                if let Err(e) = island::create_island_window(app.handle(), island_enabled) {
                    log::warn!("Failed to create island window: {}", e);
                }

                // Start island bridge server and event forwarding on the async runtime
                let island = island_state.inner().clone();
                let handle = app.handle().clone();
                let rt_handle = tauri::async_runtime::handle();
                rt_handle.spawn(async move {
                    island::setup_event_forwarding(&handle, &island.session_store);
                    island::start_bridge_server(&island);
                    island::bootstrap_provider_hooks_if_needed(&island).await;
                });

                // Start fullscreen monitor
                island::start_fullscreen_monitor(app.handle());

                #[cfg(target_os = "windows")]
                island::start_windows_island_cursor_monitor(app.handle());

                // Start screen change monitor (multi-monitor support)
                island::start_screen_change_monitor(app.handle());

                // Register global shortcuts for island
                register_island_shortcuts(app.handle());
            } else {
                log::info!(
                    "Skipping island runtime on this platform; no native island window is available"
                );
            }

            // Keep a small, visual clipboard history warm for the main window module.
            commands::clipboard::start_clipboard_monitor(
                app.handle().clone(),
                clipboard_state.clone(),
            );

            #[cfg(target_os = "macos")]
            {
                // FinderSync stays lightweight and hands heavier Finder actions to the main app.
                suppress_ready_presentation_for_setup.store(
                    commands::super_right_click::has_recent_pending_finder_action_request(),
                    Ordering::SeqCst,
                );
                commands::super_right_click::start_super_right_click_runtime_heartbeat();
                commands::super_right_click::bootstrap_super_right_click_if_enabled();
                commands::super_right_click::start_finder_action_request_worker(
                    app.handle().clone(),
                );
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::keyboard::start_interceptor,
            commands::keyboard::stop_interceptor,
            commands::keyboard::get_status,
            commands::keyboard::set_enabled,
            commands::keyboard::set_music_native_keys_suppressed,
            commands::keyboard::check_accessibility,
            commands::keyboard::request_accessibility,
            commands::keyboard::open_accessibility_settings,
            commands::keyboard::check_screen_recording,
            commands::keyboard::request_screen_recording,
            commands::keyboard::open_screen_recording_settings,
            commands::keyboard::get_alt_tab_overlay_state,
            commands::keyboard::get_bracket_overlay_state,
            commands::keyboard::select_alt_tab_window,
            commands::keyboard::set_alt_tab_hovered_window,
            commands::keyboard::activate_alt_tab_window,
            commands::keyboard::get_alt_tab_thumbnail,
            commands::keyboard::get_alt_tab_app_icon,
            commands::autostart::is_windows_autostart_enabled,
            commands::autostart::set_windows_autostart_enabled,
            commands::super_right_click::get_super_right_click_status,
            commands::super_right_click::start_super_right_click,
            commands::super_right_click::stop_super_right_click,
            commands::super_right_click::set_super_right_click_enabled,
            commands::super_right_click::set_super_right_click_terminal,
            commands::super_right_click::open_finder_sync_extension_settings,
            commands::super_right_click::open_finder_automation_settings,
            commands::clipboard::get_clipboard_history,
            commands::clipboard::get_clipboard_monitor_enabled,
            commands::clipboard::set_clipboard_monitor_enabled,
            commands::clipboard::clear_clipboard_history,
            commands::clipboard::delete_clipboard_history_item,
            commands::clipboard::set_clipboard_history_item_pinned,
            commands::clipboard::copy_clipboard_history_item,
            commands::clipboard::open_clipboard_item_location,
            commands::clipboard::open_clipboard_image_location,
            island::get_island_sessions,
            island::approve_permission,
            island::answer_question,
            island::set_island_interaction_bounds,
            island::set_island_mouse_passthrough,
            island::install_provider_hooks,
            island::uninstall_provider_hooks,
            island::get_provider_status,
            island::get_island_plugins,
            island::get_island_screen_info,
            island::set_island_enabled,
            island::get_island_enabled,
            island::jump_to_terminal,
            island::is_terminal_frontmost,
            island::get_all_screens,
            island::set_preferred_screen,
            island::get_preferred_screen,
            island::get_approval_rules,
            island::remove_approval_rule,
            island::clear_approval_rules,
            commands::terminal::get_terminal_inventory,
            commands::terminal::launch_agent_terminal,
            commands::terminal::launch_embedded_agent_terminal,
            commands::terminal::send_embedded_terminal_input,
            commands::terminal::resize_embedded_terminal,
            commands::terminal::stop_embedded_terminal,
            commands::updater::check_app_update,
            commands::updater::install_app_update,
        ])
        .on_window_event(|window, event| {
            // Prevent app from fully quitting when window is closed —
            // it should keep running in the background
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                #[cfg(any(target_os = "macos", target_os = "windows"))]
                {
                    if window.label() == MAIN_WINDOW_LABEL {
                        if let Err(error) = window.hide() {
                            log::warn!("Failed to hide the main window on close request: {error}");
                        }
                        api.prevent_close();
                    }
                }
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building Yorling");

    app.run(move |app_handle, event| match event {
        tauri::RunEvent::Ready => {
            if suppress_ready_presentation_for_run.swap(false, Ordering::SeqCst) {
                log::info!("Suppressing main window presentation for Finder action launch");
            } else if launched_from_windows_autostart() {
                log::info!("Suppressing main window presentation for Windows autostart launch");
            } else {
                present_main_window(app_handle, MainWindowLifecycle::Ready);
            }
        }
        #[cfg(target_os = "macos")]
        tauri::RunEvent::ExitRequested { .. } | tauri::RunEvent::Exit => {
            commands::super_right_click::shutdown_super_right_click_runtime();
        }
        #[cfg(target_os = "macos")]
        tauri::RunEvent::Reopen {
            has_visible_windows,
            ..
        } => present_main_window(
            app_handle,
            MainWindowLifecycle::Reopen {
                has_visible_windows,
            },
        ),
        _ => {}
    });
}

#[cfg(test)]
mod tests {
    use super::{
        MainWindowLifecycle, MainWindowPresentationPlan, main_window_presentation_plan,
        should_start_island_runtime,
    };

    #[test]
    fn island_runtime_starts_on_native_overlay_platforms() {
        assert_eq!(
            should_start_island_runtime(),
            cfg!(any(target_os = "macos", target_os = "windows"))
        );
    }

    #[test]
    fn ready_event_surfaces_the_main_window() {
        assert_eq!(
            main_window_presentation_plan(MainWindowLifecycle::Ready),
            MainWindowPresentationPlan {
                show: true,
                unminimize: true,
                focus: true,
                order_front: true,
            }
        );
    }

    #[test]
    fn dock_reopen_without_visible_windows_shows_the_main_window() {
        assert_eq!(
            main_window_presentation_plan(MainWindowLifecycle::Reopen {
                has_visible_windows: false,
            }),
            MainWindowPresentationPlan {
                show: true,
                unminimize: true,
                focus: true,
                order_front: true,
            }
        );
    }

    #[test]
    fn dock_reopen_with_visible_windows_still_brings_the_main_window_forward() {
        assert_eq!(
            main_window_presentation_plan(MainWindowLifecycle::Reopen {
                has_visible_windows: true,
            }),
            MainWindowPresentationPlan {
                show: false,
                unminimize: true,
                focus: true,
                order_front: true,
            }
        );
    }
}
