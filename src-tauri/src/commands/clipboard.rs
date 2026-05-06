use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
#[cfg(any(target_os = "macos", target_os = "windows"))]
use std::path::Path;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, State};

const CLIPBOARD_HISTORY_LIMIT: usize = 100;
const CLIPBOARD_MONITOR_INTERVAL_MS: u64 = 700;
const MAX_TEXT_CHARS: usize = 20_000;
const MAX_TEXT_PREVIEW_CHARS: usize = 700;
const MAX_IMAGE_BYTES: usize = 12 * 1024 * 1024;
const MAX_DIRECT_IMAGE_DATA_URL_BYTES: usize = 2 * 1024 * 1024;
#[cfg(target_os = "macos")]
const IMAGE_THUMBNAIL_MAX_EDGE: f64 = 720.0;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipboardHistoryItem {
    pub id: String,
    pub kind: ClipboardItemKind,
    pub preview_text: Option<String>,
    pub image_data_url: Option<String>,
    #[serde(default)]
    pub files: Vec<ClipboardFileReference>,
    #[serde(default = "default_item_count")]
    pub item_count: usize,
    pub byte_count: usize,
    pub char_count: Option<usize>,
    pub line_count: Option<usize>,
    pub width: Option<f64>,
    pub height: Option<f64>,
    pub created_at: u64,
    #[serde(default)]
    pub is_pinned: bool,
    pub source_label: Option<String>,
    pub source_path: Option<String>,
    pub hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipboardFileReference {
    pub path: String,
    pub name: String,
    pub type_label: String,
    pub byte_count: Option<u64>,
    pub is_directory: bool,
    pub is_image: bool,
    pub image_data_url: Option<String>,
    pub width: Option<f64>,
    pub height: Option<f64>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ClipboardItemKind {
    Text,
    Image,
    File,
    Group,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum ClipboardPayload {
    Text(String),
    Items(Vec<ClipboardPayloadItem>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ClipboardPayloadItem {
    file_path: Option<String>,
    image: Option<ClipboardImagePayload>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ClipboardImagePayload {
    #[serde(with = "base64_bytes")]
    bytes: Vec<u8>,
    pasteboard_type: ImagePasteboardType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
enum ImagePasteboardType {
    Png,
    Tiff,
    Jpeg,
    Heic,
    Heif,
    Gif,
    Webp,
    Bmp,
    Svg,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ClipboardStoredItem {
    summary: ClipboardHistoryItem,
    payload: ClipboardPayload,
}

#[derive(Debug)]
struct ClipboardCandidate {
    kind: ClipboardItemKind,
    preview_text: Option<String>,
    image_data_url: Option<String>,
    files: Vec<ClipboardFileReference>,
    item_count: usize,
    byte_count: usize,
    char_count: Option<usize>,
    line_count: Option<usize>,
    width: Option<f64>,
    height: Option<f64>,
    source_label: Option<String>,
    source_path: Option<String>,
    hash: String,
    payload: ClipboardPayload,
}

#[derive(Debug, Serialize, Deserialize)]
struct PersistedClipboardHistory {
    version: u8,
    next_id: u64,
    history: Vec<ClipboardStoredItem>,
}

#[derive(Debug, Clone)]
struct ImageCandidateData {
    summary_preview: Option<String>,
    file_preview: Option<String>,
    width: Option<f64>,
    height: Option<f64>,
    payload: ClipboardImagePayload,
    byte_count: usize,
    type_label: &'static str,
}

#[derive(Debug)]
struct ClipboardReadSnapshot {
    change_count: i64,
    candidate: Option<ClipboardCandidate>,
}

#[derive(Debug)]
struct ClipboardInner {
    enabled: bool,
    last_change_count: i64,
    next_id: u64,
    history: VecDeque<ClipboardStoredItem>,
}

pub struct ClipboardState {
    inner: Mutex<ClipboardInner>,
}

fn default_item_count() -> usize {
    1
}

mod base64_bytes {
    use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
    use serde::{Deserialize, Deserializer, Serializer, de};

    pub fn serialize<S>(bytes: &Vec<u8>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&BASE64_STANDARD.encode(bytes))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let encoded = String::deserialize(deserializer)?;
        BASE64_STANDARD.decode(encoded).map_err(de::Error::custom)
    }
}

impl ClipboardState {
    pub fn new() -> Self {
        let (history, next_id) = load_persisted_history();
        Self {
            inner: Mutex::new(ClipboardInner {
                enabled: true,
                last_change_count: -1,
                next_id,
                history,
            }),
        }
    }

    fn enabled(&self) -> bool {
        self.inner
            .lock()
            .map(|inner| inner.enabled)
            .unwrap_or(false)
    }

    fn set_enabled(&self, enabled: bool) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.enabled = enabled;
        }
    }

    fn last_change_count(&self) -> i64 {
        self.inner
            .lock()
            .map(|inner| inner.last_change_count)
            .unwrap_or(-1)
    }

    fn summaries(&self) -> Vec<ClipboardHistoryItem> {
        self.inner
            .lock()
            .map(|inner| {
                inner
                    .history
                    .iter()
                    .map(|item| item.summary.clone())
                    .collect()
            })
            .unwrap_or_default()
    }

    fn clear(&self) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.history.clear();
            persist_history(&inner);
        }
    }

    fn payload_for(&self, id: &str) -> Option<ClipboardPayload> {
        self.inner.lock().ok().and_then(|inner| {
            inner
                .history
                .iter()
                .find(|item| item.summary.id == id)
                .map(|item| item.payload.clone())
        })
    }

    fn source_path_for(&self, id: &str) -> Option<String> {
        self.inner.lock().ok().and_then(|inner| {
            inner
                .history
                .iter()
                .find(|item| item.summary.id == id)
                .and_then(|item| item.summary.source_path.clone())
        })
    }

    fn promote_existing(&self, id: &str) {
        if let Ok(mut inner) = self.inner.lock() {
            let Some(position) = inner.history.iter().position(|item| item.summary.id == id) else {
                return;
            };

            if let Some(mut stored) = inner.history.remove(position) {
                stored.summary.created_at = now_millis();
                insert_stored_item(&mut inner.history, stored);
                persist_history(&inner);
            }
        }
    }

    fn delete(&self, id: &str) -> bool {
        if let Ok(mut inner) = self.inner.lock() {
            let Some(position) = inner.history.iter().position(|item| item.summary.id == id) else {
                return false;
            };
            inner.history.remove(position);
            persist_history(&inner);
            return true;
        }

        false
    }

    fn set_pinned(&self, id: &str, pinned: bool) -> bool {
        if let Ok(mut inner) = self.inner.lock() {
            let Some(position) = inner.history.iter().position(|item| item.summary.id == id) else {
                return false;
            };

            if let Some(mut stored) = inner.history.remove(position) {
                stored.summary.is_pinned = pinned;
                insert_stored_item(&mut inner.history, stored);
                persist_history(&inner);
                return true;
            }
        }

        false
    }

    fn apply_snapshot(&self, snapshot: ClipboardReadSnapshot) -> bool {
        let Some(candidate) = snapshot.candidate else {
            if let Ok(mut inner) = self.inner.lock() {
                inner.last_change_count = snapshot.change_count;
            }
            return false;
        };

        if let Ok(mut inner) = self.inner.lock() {
            inner.last_change_count = snapshot.change_count;

            if let Some(position) = inner
                .history
                .iter()
                .position(|item| item.summary.hash == candidate.hash)
            {
                if let Some(mut stored) = inner.history.remove(position) {
                    stored.summary.created_at = now_millis();
                    if candidate.source_path.is_some() {
                        stored.summary.source_path = candidate.source_path;
                    }
                    if !candidate.files.is_empty() {
                        stored.summary.files = candidate.files;
                    }
                    stored.summary.item_count = candidate.item_count;
                    insert_stored_item(&mut inner.history, stored);
                    persist_history(&inner);
                    return true;
                }
            }

            let id = format!("clip-{}", inner.next_id);
            inner.next_id += 1;
            insert_stored_item(
                &mut inner.history,
                ClipboardStoredItem {
                    summary: ClipboardHistoryItem {
                        id,
                        kind: candidate.kind,
                        preview_text: candidate.preview_text,
                        image_data_url: candidate.image_data_url,
                        files: candidate.files,
                        item_count: candidate.item_count,
                        byte_count: candidate.byte_count,
                        char_count: candidate.char_count,
                        line_count: candidate.line_count,
                        width: candidate.width,
                        height: candidate.height,
                        created_at: now_millis(),
                        is_pinned: false,
                        source_label: candidate.source_label,
                        source_path: candidate.source_path,
                        hash: candidate.hash,
                    },
                    payload: candidate.payload,
                },
            );

            while inner.history.len() > CLIPBOARD_HISTORY_LIMIT {
                if let Some(last_unpinned_position) = inner
                    .history
                    .iter()
                    .rposition(|item| !item.summary.is_pinned)
                {
                    inner.history.remove(last_unpinned_position);
                } else {
                    inner.history.pop_back();
                }
            }

            persist_history(&inner);
            true
        } else {
            false
        }
    }
}

impl Default for ClipboardState {
    fn default() -> Self {
        Self::new()
    }
}

pub fn start_clipboard_monitor(app_handle: AppHandle, state: Arc<ClipboardState>) {
    tauri::async_runtime::spawn(async move {
        loop {
            if state.enabled() {
                match poll_clipboard_once(&app_handle, &state) {
                    Ok(true) => {
                        let _ = app_handle.emit("clipboard-history-updated", ());
                    }
                    Ok(false) => {}
                    Err(error) => {
                        log::debug!("Clipboard monitor skipped update: {error}");
                    }
                }
            }

            tokio::time::sleep(Duration::from_millis(CLIPBOARD_MONITOR_INTERVAL_MS)).await;
        }
    });
}

#[tauri::command]
pub async fn get_clipboard_history(
    app_handle: AppHandle,
    state: State<'_, Arc<ClipboardState>>,
) -> Result<Vec<ClipboardHistoryItem>, String> {
    if state.enabled() {
        let _ = poll_clipboard_once(&app_handle, state.inner());
    }
    Ok(state.summaries())
}

#[tauri::command]
pub async fn get_clipboard_monitor_enabled(
    state: State<'_, Arc<ClipboardState>>,
) -> Result<bool, String> {
    Ok(state.enabled())
}

#[tauri::command]
pub async fn set_clipboard_monitor_enabled(
    state: State<'_, Arc<ClipboardState>>,
    enabled: bool,
) -> Result<(), String> {
    state.set_enabled(enabled);
    Ok(())
}

#[tauri::command]
pub async fn clear_clipboard_history(state: State<'_, Arc<ClipboardState>>) -> Result<(), String> {
    state.clear();
    Ok(())
}

#[tauri::command]
pub async fn delete_clipboard_history_item(
    state: State<'_, Arc<ClipboardState>>,
    id: String,
) -> Result<(), String> {
    state
        .delete(&id)
        .then_some(())
        .ok_or_else(|| "Clipboard item was not found".to_string())
}

#[tauri::command]
pub async fn set_clipboard_history_item_pinned(
    state: State<'_, Arc<ClipboardState>>,
    id: String,
    pinned: bool,
) -> Result<(), String> {
    state
        .set_pinned(&id, pinned)
        .then_some(())
        .ok_or_else(|| "Clipboard item was not found".to_string())
}

#[tauri::command]
pub async fn copy_clipboard_history_item(
    app_handle: AppHandle,
    state: State<'_, Arc<ClipboardState>>,
    id: String,
) -> Result<(), String> {
    let payload = state
        .payload_for(&id)
        .ok_or_else(|| "Clipboard item was not found".to_string())?;

    write_payload_to_pasteboard(&app_handle, payload)?;
    state.promote_existing(&id);
    Ok(())
}

#[tauri::command]
pub async fn open_clipboard_item_location(
    state: State<'_, Arc<ClipboardState>>,
    id: String,
) -> Result<(), String> {
    let source_path = state
        .source_path_for(&id)
        .ok_or_else(|| "This clipboard item does not have a local file location".to_string())?;

    reveal_path_in_file_manager(&source_path)
}

#[tauri::command]
pub async fn open_clipboard_image_location(
    state: State<'_, Arc<ClipboardState>>,
    id: String,
) -> Result<(), String> {
    open_clipboard_item_location(state, id).await
}

fn poll_clipboard_once(
    app_handle: &AppHandle,
    state: &Arc<ClipboardState>,
) -> Result<bool, String> {
    let snapshot = read_clipboard_snapshot(app_handle, state.last_change_count())?;
    Ok(state.apply_snapshot(snapshot))
}

fn insert_stored_item(history: &mut VecDeque<ClipboardStoredItem>, stored: ClipboardStoredItem) {
    if stored.summary.is_pinned {
        history.push_front(stored);
        return;
    }

    let insertion_index = history
        .iter()
        .position(|item| !item.summary.is_pinned)
        .unwrap_or(history.len());
    history.insert(insertion_index, stored);
}

fn clipboard_history_path() -> Option<PathBuf> {
    dirs::data_dir().map(|directory| directory.join("Yorling").join("clipboard-history.json"))
}

fn load_persisted_history() -> (VecDeque<ClipboardStoredItem>, u64) {
    let Some(path) = clipboard_history_path() else {
        return (VecDeque::new(), 1);
    };

    let Ok(contents) = fs::read_to_string(&path) else {
        return (VecDeque::new(), 1);
    };

    let Ok(mut persisted) = serde_json::from_str::<PersistedClipboardHistory>(&contents) else {
        log::warn!(
            "Failed to parse persisted clipboard history at {}",
            path.display()
        );
        return (VecDeque::new(), 1);
    };

    persisted.history.truncate(CLIPBOARD_HISTORY_LIMIT);
    let next_from_ids = persisted
        .history
        .iter()
        .filter_map(|item| item.summary.id.strip_prefix("clip-"))
        .filter_map(|value| value.parse::<u64>().ok())
        .max()
        .map(|value| value + 1)
        .unwrap_or(1);
    let next_id = persisted.next_id.max(next_from_ids).max(1);
    (VecDeque::from(persisted.history), next_id)
}

fn persist_history(inner: &ClipboardInner) {
    let Some(path) = clipboard_history_path() else {
        return;
    };

    if let Some(parent) = path.parent() {
        if let Err(error) = fs::create_dir_all(parent) {
            log::warn!("Failed to create clipboard history directory: {error}");
            return;
        }
    }

    let persisted = PersistedClipboardHistory {
        version: 1,
        next_id: inner.next_id,
        history: inner.history.iter().cloned().collect(),
    };

    match serde_json::to_vec_pretty(&persisted) {
        Ok(bytes) => {
            if let Err(error) = fs::write(&path, bytes) {
                log::warn!(
                    "Failed to persist clipboard history to {}: {error}",
                    path.display()
                );
            }
        }
        Err(error) => {
            log::warn!("Failed to serialize clipboard history: {error}");
        }
    }
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(target_os = "macos")]
fn reveal_path_in_file_manager(path: &str) -> Result<(), String> {
    let file_path = Path::new(path);
    if !file_path.exists() {
        return Err("The original file no longer exists".into());
    }

    let status = std::process::Command::new("open")
        .arg("-R")
        .arg(file_path)
        .status()
        .map_err(|error| format!("Failed to ask Finder to reveal the file: {error}"))?;

    if status.success() {
        Ok(())
    } else {
        Err(format!("Finder returned status {status}"))
    }
}

#[cfg(target_os = "windows")]
fn reveal_path_in_file_manager(path: &str) -> Result<(), String> {
    let file_path = Path::new(path);
    if !file_path.exists() {
        return Err("The original file no longer exists".into());
    }

    let target = if file_path.is_dir() {
        path.to_string()
    } else {
        format!("/select,{path}")
    };

    let status = std::process::Command::new("explorer")
        .arg(target)
        .status()
        .map_err(|error| format!("Failed to ask Explorer to reveal the file: {error}"))?;

    if status.success() {
        Ok(())
    } else {
        Err(format!("Explorer returned status {status}"))
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn reveal_path_in_file_manager(_path: &str) -> Result<(), String> {
    Err("Opening image locations is not supported on this platform yet".into())
}

fn hash_parts(kind: ClipboardItemKind, bytes: &[u8]) -> String {
    let mut hasher = DefaultHasher::new();
    match kind {
        ClipboardItemKind::Text => "text".hash(&mut hasher),
        ClipboardItemKind::Image => "image".hash(&mut hasher),
        ClipboardItemKind::File => "file".hash(&mut hasher),
        ClipboardItemKind::Group => "group".hash(&mut hasher),
    }
    bytes.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn hash_payload_items(kind: ClipboardItemKind, items: &[ClipboardPayloadItem]) -> String {
    let mut hasher = DefaultHasher::new();
    match kind {
        ClipboardItemKind::Text => "text".hash(&mut hasher),
        ClipboardItemKind::Image => "image".hash(&mut hasher),
        ClipboardItemKind::File => "file".hash(&mut hasher),
        ClipboardItemKind::Group => "group".hash(&mut hasher),
    }
    for item in items {
        item.file_path.hash(&mut hasher);
        if let Some(image) = &item.image {
            image.pasteboard_type.hash(&mut hasher);
            image.bytes.hash(&mut hasher);
        }
    }
    format!("{:016x}", hasher.finish())
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    let mut truncated: String = value.chars().take(max_chars).collect();
    if value.chars().count() > max_chars {
        truncated.push('…');
    }
    truncated
}

#[cfg(target_os = "macos")]
fn read_clipboard_snapshot(
    app_handle: &AppHandle,
    last_change_count: i64,
) -> Result<ClipboardReadSnapshot, String> {
    use std::sync::mpsc;

    let (sender, receiver) = mpsc::channel();
    app_handle
        .run_on_main_thread(move || {
            let _ = sender.send(platform::read_clipboard_snapshot(last_change_count));
        })
        .map_err(|error| format!("Failed to read clipboard on the main thread: {error}"))?;

    receiver
        .recv_timeout(Duration::from_secs(2))
        .map_err(|error| format!("Timed out while reading clipboard: {error}"))?
}

#[cfg(target_os = "windows")]
fn read_clipboard_snapshot(
    _app_handle: &AppHandle,
    last_change_count: i64,
) -> Result<ClipboardReadSnapshot, String> {
    platform::read_clipboard_snapshot(last_change_count)
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn read_clipboard_snapshot(
    _app_handle: &AppHandle,
    last_change_count: i64,
) -> Result<ClipboardReadSnapshot, String> {
    Ok(ClipboardReadSnapshot {
        change_count: last_change_count,
        candidate: None,
    })
}

#[cfg(target_os = "macos")]
fn write_payload_to_pasteboard(
    app_handle: &AppHandle,
    payload: ClipboardPayload,
) -> Result<(), String> {
    use std::sync::mpsc;

    let (sender, receiver) = mpsc::channel();
    app_handle
        .run_on_main_thread(move || {
            let _ = sender.send(platform::write_payload_to_pasteboard(payload));
        })
        .map_err(|error| format!("Failed to write clipboard on the main thread: {error}"))?;

    receiver
        .recv_timeout(Duration::from_secs(2))
        .map_err(|error| format!("Timed out while writing clipboard: {error}"))?
}

#[cfg(target_os = "windows")]
fn write_payload_to_pasteboard(
    _app_handle: &AppHandle,
    payload: ClipboardPayload,
) -> Result<(), String> {
    platform::write_payload_to_pasteboard(payload)
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn write_payload_to_pasteboard(
    _app_handle: &AppHandle,
    _payload: ClipboardPayload,
) -> Result<(), String> {
    Err("Clipboard history is not supported on this platform yet".into())
}

#[cfg(target_os = "macos")]
mod platform {
    use super::{
        ClipboardCandidate, ClipboardFileReference, ClipboardImagePayload, ClipboardItemKind,
        ClipboardPayload, ClipboardPayloadItem, ClipboardReadSnapshot, IMAGE_THUMBNAIL_MAX_EDGE,
        ImageCandidateData, ImagePasteboardType, MAX_DIRECT_IMAGE_DATA_URL_BYTES, MAX_IMAGE_BYTES,
        MAX_TEXT_CHARS, MAX_TEXT_PREVIEW_CHARS, hash_parts, hash_payload_items, truncate_chars,
    };
    use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
    use objc2::AnyThread;
    use objc2::runtime::{AnyObject, ProtocolObject};
    use objc2_app_kit::{
        NSBitmapImageFileType, NSBitmapImageRep, NSCompositingOperation, NSImage, NSPasteboard,
        NSPasteboardItem, NSPasteboardTypeFileURL, NSPasteboardTypePNG, NSPasteboardTypeString,
        NSPasteboardTypeTIFF, NSPasteboardWriting,
    };
    use objc2_foundation::{
        NSArray, NSData, NSDictionary, NSPoint, NSRect, NSSize, NSString, NSURL,
    };
    use std::path::Path;

    pub(super) fn read_clipboard_snapshot(
        last_change_count: i64,
    ) -> Result<ClipboardReadSnapshot, String> {
        let pasteboard = NSPasteboard::generalPasteboard();
        let change_count = pasteboard.changeCount() as i64;

        if change_count == last_change_count {
            return Ok(ClipboardReadSnapshot {
                change_count,
                candidate: None,
            });
        }

        let types = pasteboard_types(&pasteboard);
        if types.is_empty() || is_transient_or_confidential(&types) {
            return Ok(ClipboardReadSnapshot {
                change_count,
                candidate: None,
            });
        }

        let candidate = read_file_items_candidate(&pasteboard)
            .or_else(|| read_image_items_candidate(&pasteboard))
            .or_else(|| read_text_candidate(&pasteboard));

        Ok(ClipboardReadSnapshot {
            change_count,
            candidate,
        })
    }

    pub(super) fn write_payload_to_pasteboard(payload: ClipboardPayload) -> Result<(), String> {
        let pasteboard = NSPasteboard::generalPasteboard();
        pasteboard.clearContents();

        match payload {
            ClipboardPayload::Text(text) => {
                let string = objc2_foundation::NSString::from_str(&text);
                if pasteboard.setString_forType(&string, unsafe { NSPasteboardTypeString }) {
                    Ok(())
                } else {
                    Err("macOS rejected the text clipboard write".into())
                }
            }
            ClipboardPayload::Items(items) => {
                let pasteboard_items = items
                    .iter()
                    .filter_map(pasteboard_item_from_payload_item)
                    .collect::<Vec<_>>();

                if pasteboard_items.is_empty() {
                    return Err("Clipboard item has no writable payload".into());
                }

                let array = NSArray::from_retained_slice(&pasteboard_items);
                let writable_objects =
                    unsafe { array.cast_unchecked::<ProtocolObject<dyn NSPasteboardWriting>>() };

                if pasteboard.writeObjects(writable_objects) {
                    Ok(())
                } else {
                    Err("macOS rejected the clipboard item write".into())
                }
            }
        }
    }

    fn pasteboard_item_from_payload_item(
        payload_item: &ClipboardPayloadItem,
    ) -> Option<objc2::rc::Retained<NSPasteboardItem>> {
        let item = NSPasteboardItem::new();
        let mut wrote_any = false;

        if let Some(path) = &payload_item.file_path {
            wrote_any |= write_file_url_to_item(&item, path);
        }

        if let Some(image) = &payload_item.image {
            wrote_any |= write_image_to_item(&item, image);
        }

        wrote_any.then_some(item)
    }

    fn write_file_url_to_item(item: &NSPasteboardItem, path: &str) -> bool {
        let path_string = NSString::from_str(path);
        let is_directory = std::fs::metadata(path)
            .map(|metadata| metadata.is_dir())
            .unwrap_or(false);
        let url = NSURL::fileURLWithPath_isDirectory(&path_string, is_directory);
        let data = url.dataRepresentation();
        let wrote_data = item.setData_forType(&data, unsafe { NSPasteboardTypeFileURL });
        let wrote_string = url
            .absoluteString()
            .map(|absolute_string| {
                item.setString_forType(&absolute_string, unsafe { NSPasteboardTypeFileURL })
            })
            .unwrap_or(false);

        wrote_data || wrote_string
    }

    fn write_image_to_item(item: &NSPasteboardItem, image: &ClipboardImagePayload) -> bool {
        let data = NSData::with_bytes(&image.bytes);
        let custom_data_type;
        let data_type = match image.pasteboard_type {
            ImagePasteboardType::Png => unsafe { NSPasteboardTypePNG },
            ImagePasteboardType::Tiff => unsafe { NSPasteboardTypeTIFF },
            ImagePasteboardType::Jpeg => {
                custom_data_type = NSString::from_str("public.jpeg");
                &custom_data_type
            }
            ImagePasteboardType::Heic => {
                custom_data_type = NSString::from_str("public.heic");
                &custom_data_type
            }
            ImagePasteboardType::Heif => {
                custom_data_type = NSString::from_str("public.heif");
                &custom_data_type
            }
            ImagePasteboardType::Gif => {
                custom_data_type = NSString::from_str("com.compuserve.gif");
                &custom_data_type
            }
            ImagePasteboardType::Webp => {
                custom_data_type = NSString::from_str("org.webmproject.webp");
                &custom_data_type
            }
            ImagePasteboardType::Bmp => {
                custom_data_type = NSString::from_str("com.microsoft.bmp");
                &custom_data_type
            }
            ImagePasteboardType::Svg => {
                custom_data_type = NSString::from_str("public.svg-image");
                &custom_data_type
            }
        };

        item.setData_forType(&data, data_type)
    }

    fn pasteboard_types(pasteboard: &NSPasteboard) -> Vec<String> {
        pasteboard
            .types()
            .map(|types| {
                types
                    .to_vec()
                    .into_iter()
                    .map(|item| item.to_string())
                    .collect()
            })
            .unwrap_or_default()
    }

    fn is_transient_or_confidential(types: &[String]) -> bool {
        const SKIPPED_TYPES: &[&str] = &[
            "org.nspasteboard.ConcealedType",
            "org.nspasteboard.TransientType",
            "org.nspasteboard.AutoGeneratedType",
            "de.petermaurer.TransientPasteboardType",
            "com.typeit4me.clipping",
            "Pasteboard generator type",
            "com.agilebits.onepassword",
            "net.antelle.keeweb",
        ];

        types
            .iter()
            .any(|pasteboard_type| SKIPPED_TYPES.contains(&pasteboard_type.as_str()))
    }

    fn read_text_candidate(pasteboard: &NSPasteboard) -> Option<ClipboardCandidate> {
        let text = pasteboard
            .stringForType(unsafe { NSPasteboardTypeString })?
            .to_string();

        if text.trim().is_empty() {
            return None;
        }

        let bytes = text.as_bytes();
        let char_count = text.chars().count();
        let line_count = text.lines().count().max(1);
        let stored_text = truncate_chars(&text, MAX_TEXT_CHARS);
        let preview_text = truncate_chars(&text, MAX_TEXT_PREVIEW_CHARS);

        Some(ClipboardCandidate {
            kind: ClipboardItemKind::Text,
            preview_text: Some(preview_text),
            image_data_url: None,
            files: Vec::new(),
            item_count: 1,
            byte_count: bytes.len(),
            char_count: Some(char_count),
            line_count: Some(line_count),
            width: None,
            height: None,
            source_label: Some("TEXT".into()),
            source_path: None,
            hash: hash_parts(ClipboardItemKind::Text, bytes),
            payload: ClipboardPayload::Text(stored_text),
        })
    }

    fn read_file_items_candidate(pasteboard: &NSPasteboard) -> Option<ClipboardCandidate> {
        let paths = file_paths_from_pasteboard(pasteboard);
        if paths.is_empty() {
            return None;
        }

        let mut files = Vec::with_capacity(paths.len());
        let mut payload_items = Vec::with_capacity(paths.len());
        let mut total_bytes: u64 = 0;
        let mut image_count = 0;
        let mut first_preview = None;
        let mut width = None;
        let mut height = None;

        for path in &paths {
            let image_data = image_candidate_from_file_path(path);
            let file = file_reference_from_path(path, image_data.as_ref());

            if file.is_image {
                image_count += 1;
                if first_preview.is_none() {
                    first_preview = image_data
                        .as_ref()
                        .and_then(|image| image.summary_preview.clone());
                    width = image_data.as_ref().and_then(|image| image.width);
                    height = image_data.as_ref().and_then(|image| image.height);
                }
            }

            if let Some(byte_count) = file.byte_count {
                total_bytes = total_bytes.saturating_add(byte_count);
            }

            payload_items.push(ClipboardPayloadItem {
                file_path: Some(path.clone()),
                image: image_data.map(|image| image.payload),
            });
            files.push(file);
        }

        let kind = if paths.len() == 1 {
            if files.first().map(|file| file.is_image).unwrap_or(false) {
                ClipboardItemKind::Image
            } else {
                ClipboardItemKind::File
            }
        } else {
            ClipboardItemKind::Group
        };

        let source_label = file_source_label(&files, image_count);
        let preview_text = file_preview_text(&files);
        let byte_count = usize::try_from(total_bytes).unwrap_or(usize::MAX);
        let hash = hash_payload_items(kind, &payload_items);

        Some(ClipboardCandidate {
            kind,
            preview_text,
            image_data_url: first_preview,
            files,
            item_count: payload_items.len(),
            byte_count,
            char_count: None,
            line_count: None,
            width,
            height,
            source_label: Some(source_label),
            source_path: paths.first().cloned(),
            hash,
            payload: ClipboardPayload::Items(payload_items),
        })
    }

    fn read_image_items_candidate(pasteboard: &NSPasteboard) -> Option<ClipboardCandidate> {
        let images = image_items_from_pasteboard(pasteboard);
        if images.is_empty() {
            return None;
        }

        let kind = if images.len() == 1 {
            ClipboardItemKind::Image
        } else {
            ClipboardItemKind::Group
        };
        let byte_count = images.iter().fold(0usize, |total, image| {
            total.saturating_add(image.byte_count)
        });
        let first = images.first()?;
        let image_data_url = first.summary_preview.clone();
        let width = (images.len() == 1).then_some(first.width).flatten();
        let height = (images.len() == 1).then_some(first.height).flatten();
        let source_label = if images.len() == 1 {
            first.type_label.to_string()
        } else {
            format!("{} IMAGES", images.len())
        };
        let preview_text = (images.len() > 1).then(|| format!("{} images", images.len()));
        let payload_items = images
            .into_iter()
            .map(|image| ClipboardPayloadItem {
                file_path: None,
                image: Some(image.payload),
            })
            .collect::<Vec<_>>();
        let hash = hash_payload_items(kind, &payload_items);

        Some(ClipboardCandidate {
            kind,
            preview_text,
            image_data_url,
            files: Vec::new(),
            item_count: payload_items.len(),
            byte_count,
            char_count: None,
            line_count: None,
            width,
            height,
            source_label: Some(source_label),
            source_path: None,
            hash,
            payload: ClipboardPayload::Items(payload_items),
        })
    }

    fn data_url(mime_type: &str, bytes: &[u8]) -> String {
        format!("data:{mime_type};base64,{}", BASE64_STANDARD.encode(bytes))
    }

    fn file_paths_from_pasteboard(pasteboard: &NSPasteboard) -> Vec<String> {
        let mut paths = Vec::new();
        if let Some(items) = pasteboard.pasteboardItems() {
            for item in items.to_vec() {
                if let Some(path) = file_path_from_item(&item) {
                    push_unique_path(&mut paths, path);
                }
            }
        }

        if let Some(path) = file_path_from_pasteboard_root(pasteboard) {
            push_unique_path(&mut paths, path);
        }

        paths
    }

    fn push_unique_path(paths: &mut Vec<String>, path: String) {
        if !paths.iter().any(|existing| existing == &path) {
            paths.push(path);
        }
    }

    fn file_path_from_item(item: &NSPasteboardItem) -> Option<String> {
        item.dataForType(unsafe { NSPasteboardTypeFileURL })
            .and_then(|data| file_url_data_to_path(&data))
            .or_else(|| {
                item.stringForType(unsafe { NSPasteboardTypeFileURL })
                    .and_then(|value| file_url_string_to_path(&value.to_string()))
            })
    }

    fn file_path_from_pasteboard_root(pasteboard: &NSPasteboard) -> Option<String> {
        pasteboard
            .dataForType(unsafe { NSPasteboardTypeFileURL })
            .and_then(|data| file_url_data_to_path(&data))
            .or_else(|| {
                pasteboard
                    .stringForType(unsafe { NSPasteboardTypeFileURL })
                    .and_then(|value| file_url_string_to_path(&value.to_string()))
            })
    }

    fn file_url_data_to_path(data: &NSData) -> Option<String> {
        let url = NSURL::URLWithDataRepresentation_relativeToURL(data, None);
        file_url_to_path_from_nsurl(&url).or_else(|| {
            String::from_utf8(data.to_vec())
                .ok()
                .and_then(|value| file_url_string_to_path(&value))
        })
    }

    fn file_url_string_to_path(file_url: &str) -> Option<String> {
        file_url_to_path(file_url).or_else(|| {
            let trimmed = file_url.trim();
            Path::new(trimmed)
                .is_absolute()
                .then(|| trimmed.to_string())
        })
    }

    fn file_url_to_path_from_nsurl(url: &NSURL) -> Option<String> {
        if !url.isFileURL() {
            return None;
        }

        Some(url.to_file_path()?.to_string_lossy().into_owned())
    }

    fn file_reference_from_path(
        path: &str,
        image_data: Option<&ImageCandidateData>,
    ) -> ClipboardFileReference {
        let path_ref = Path::new(path);
        let metadata = std::fs::metadata(path).ok();
        let is_directory = metadata
            .as_ref()
            .map(|metadata| metadata.is_dir())
            .unwrap_or(false);
        let is_image = !is_directory && image_pasteboard_type_from_path(path).is_some();
        let name = path_ref
            .file_name()
            .map(|value| value.to_string_lossy().into_owned())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| path.to_string());
        let type_label = file_type_label(path_ref, is_directory);

        ClipboardFileReference {
            path: path.to_string(),
            name,
            type_label,
            byte_count: metadata
                .as_ref()
                .and_then(|metadata| (!metadata.is_dir()).then_some(metadata.len())),
            is_directory,
            is_image,
            image_data_url: image_data.and_then(|image| image.file_preview.clone()),
            width: image_data.and_then(|image| image.width),
            height: image_data.and_then(|image| image.height),
        }
    }

    fn file_type_label(path: &Path, is_directory: bool) -> String {
        if let Some(extension) = path.extension().and_then(|value| value.to_str()) {
            let extension = extension.trim();
            if !extension.is_empty() {
                return extension.to_ascii_uppercase();
            }
        }

        if is_directory {
            "FOLDER".into()
        } else {
            "FILE".into()
        }
    }

    fn file_source_label(files: &[ClipboardFileReference], image_count: usize) -> String {
        match files {
            [single] if single.is_directory => "FOLDER".into(),
            [single] => single.type_label.clone(),
            _ if image_count == files.len() => format!("{} IMAGES", files.len()),
            _ => format!("{} FILES", files.len()),
        }
    }

    fn file_preview_text(files: &[ClipboardFileReference]) -> Option<String> {
        if files.is_empty() {
            return None;
        }

        let mut names = files
            .iter()
            .take(4)
            .map(|file| file.name.as_str())
            .collect::<Vec<_>>()
            .join("\n");

        if files.len() > 4 {
            names.push_str(&format!("\n+{} more", files.len() - 4));
        }

        Some(names)
    }

    fn image_items_from_pasteboard(pasteboard: &NSPasteboard) -> Vec<ImageCandidateData> {
        let mut images = Vec::new();
        if let Some(items) = pasteboard.pasteboardItems() {
            for item in items.to_vec() {
                if let Some(image) = image_candidate_from_item(&item) {
                    images.push(image);
                }
            }
        }

        if images.is_empty() {
            if let Some(image) = image_candidate_from_pasteboard_root(pasteboard) {
                images.push(image);
            }
        }

        images
    }

    fn image_candidate_from_item(item: &NSPasteboardItem) -> Option<ImageCandidateData> {
        item.dataForType(unsafe { NSPasteboardTypePNG })
            .and_then(|data| image_candidate_from_data(ImagePasteboardType::Png, data.to_vec()))
            .or_else(|| {
                item.dataForType(unsafe { NSPasteboardTypeTIFF })
                    .and_then(|data| {
                        image_candidate_from_data(ImagePasteboardType::Tiff, data.to_vec())
                    })
            })
    }

    fn image_candidate_from_pasteboard_root(
        pasteboard: &NSPasteboard,
    ) -> Option<ImageCandidateData> {
        pasteboard
            .dataForType(unsafe { NSPasteboardTypePNG })
            .and_then(|data| image_candidate_from_data(ImagePasteboardType::Png, data.to_vec()))
            .or_else(|| {
                pasteboard
                    .dataForType(unsafe { NSPasteboardTypeTIFF })
                    .and_then(|data| {
                        image_candidate_from_data(ImagePasteboardType::Tiff, data.to_vec())
                    })
            })
    }

    fn image_candidate_from_file_path(path: &str) -> Option<ImageCandidateData> {
        let pasteboard_type = image_pasteboard_type_from_path(path)?;
        let bytes = std::fs::read(path).ok()?;
        image_candidate_from_data(pasteboard_type, bytes)
    }

    fn image_candidate_from_data(
        pasteboard_type: ImagePasteboardType,
        bytes: Vec<u8>,
    ) -> Option<ImageCandidateData> {
        if bytes.is_empty() || bytes.len() > MAX_IMAGE_BYTES {
            return None;
        }

        if pasteboard_type == ImagePasteboardType::Svg {
            let (width, height) = svg_dimensions_from_bytes(&bytes);
            let preview = data_url("image/svg+xml", &bytes);
            let byte_count = bytes.len();
            return Some(ImageCandidateData {
                summary_preview: Some(preview.clone()),
                file_preview: Some(preview),
                width,
                height,
                payload: ClipboardImagePayload {
                    bytes,
                    pasteboard_type,
                },
                byte_count,
                type_label: image_type_label(pasteboard_type),
            });
        }

        let image = image_from_bytes(&bytes)?;
        let size = image.size();
        let preview_bytes = image_preview_png_bytes(&image, &bytes, pasteboard_type)?;
        let preview = data_url("image/png", &preview_bytes);
        let byte_count = bytes.len();

        Some(ImageCandidateData {
            summary_preview: Some(preview.clone()),
            file_preview: Some(preview),
            width: Some(size.width),
            height: Some(size.height),
            payload: ClipboardImagePayload {
                bytes,
                pasteboard_type,
            },
            byte_count,
            type_label: image_type_label(pasteboard_type),
        })
    }

    fn image_pasteboard_type_from_path(path: &str) -> Option<ImagePasteboardType> {
        let extension = path.rsplit('.').next()?.to_ascii_lowercase();
        match extension.as_str() {
            "png" => Some(ImagePasteboardType::Png),
            "jpg" | "jpeg" => Some(ImagePasteboardType::Jpeg),
            "tif" | "tiff" => Some(ImagePasteboardType::Tiff),
            "heic" => Some(ImagePasteboardType::Heic),
            "heif" => Some(ImagePasteboardType::Heif),
            "gif" => Some(ImagePasteboardType::Gif),
            "webp" => Some(ImagePasteboardType::Webp),
            "bmp" => Some(ImagePasteboardType::Bmp),
            "svg" => Some(ImagePasteboardType::Svg),
            _ => None,
        }
    }

    fn image_type_label(pasteboard_type: ImagePasteboardType) -> &'static str {
        match pasteboard_type {
            ImagePasteboardType::Png => "PNG",
            ImagePasteboardType::Tiff => "TIFF",
            ImagePasteboardType::Jpeg => "JPEG",
            ImagePasteboardType::Heic => "HEIC",
            ImagePasteboardType::Heif => "HEIF",
            ImagePasteboardType::Gif => "GIF",
            ImagePasteboardType::Webp => "WEBP",
            ImagePasteboardType::Bmp => "BMP",
            ImagePasteboardType::Svg => "SVG",
        }
    }

    fn file_url_to_path(file_url: &str) -> Option<String> {
        let url = file_url.trim();
        let without_scheme = url.strip_prefix("file://")?;
        let path_part = without_scheme
            .strip_prefix("localhost/")
            .map(|path| format!("/{path}"))
            .unwrap_or_else(|| {
                if without_scheme.starts_with('/') {
                    without_scheme.to_string()
                } else {
                    format!("/{without_scheme}")
                }
            });

        percent_decode_path(&path_part)
    }

    fn percent_decode_path(value: &str) -> Option<String> {
        let bytes = value.as_bytes();
        let mut output = Vec::with_capacity(bytes.len());
        let mut index = 0;

        while index < bytes.len() {
            if bytes[index] == b'%' {
                let high = bytes.get(index + 1).and_then(|value| hex_value(*value))?;
                let low = bytes.get(index + 2).and_then(|value| hex_value(*value))?;
                output.push((high << 4) | low);
                index += 3;
            } else {
                output.push(bytes[index]);
                index += 1;
            }
        }

        String::from_utf8(output).ok()
    }

    fn hex_value(value: u8) -> Option<u8> {
        match value {
            b'0'..=b'9' => Some(value - b'0'),
            b'a'..=b'f' => Some(value - b'a' + 10),
            b'A'..=b'F' => Some(value - b'A' + 10),
            _ => None,
        }
    }

    fn svg_dimensions_from_bytes(bytes: &[u8]) -> (Option<f64>, Option<f64>) {
        let Ok(text) = std::str::from_utf8(bytes) else {
            return (None, None);
        };

        let Some(svg_tag) = svg_opening_tag(text) else {
            return (None, None);
        };

        let width = svg_attribute(&svg_tag, "width").and_then(parse_svg_length);
        let height = svg_attribute(&svg_tag, "height").and_then(parse_svg_length);
        if width.is_some() && height.is_some() {
            return (width, height);
        }

        let view_box = svg_attribute(&svg_tag, "viewBox")
            .or_else(|| svg_attribute(&svg_tag, "viewbox"))
            .and_then(parse_svg_view_box);

        match view_box {
            Some((view_box_width, view_box_height)) => (
                width.or(Some(view_box_width)),
                height.or(Some(view_box_height)),
            ),
            None => (width, height),
        }
    }

    fn svg_opening_tag(text: &str) -> Option<String> {
        let start = text.find("<svg")?;
        let end = text[start..].find('>')?;
        Some(text[start..start + end].to_string())
    }

    fn svg_attribute(tag: &str, name: &str) -> Option<String> {
        let mut remaining = tag;
        while let Some(offset) = remaining.find(name) {
            let name_start = tag.len() - remaining.len() + offset;
            let before_name = tag[..name_start].chars().next_back();
            let after_name = &tag[name_start + name.len()..];

            let starts_attribute = before_name
                .map(|character| character.is_whitespace() || character == '<')
                .unwrap_or(false);
            if starts_attribute && after_name.trim_start().starts_with('=') {
                let after_equals = after_name.trim_start().strip_prefix('=')?.trim_start();
                let quote = after_equals.chars().next()?;
                if quote != '"' && quote != '\'' {
                    return None;
                }
                let value_start = quote.len_utf8();
                let value_end = after_equals[value_start..].find(quote)?;
                return Some(after_equals[value_start..value_start + value_end].to_string());
            }

            remaining = &after_name[after_name
                .char_indices()
                .nth(1)
                .map(|(index, _)| index)
                .unwrap_or(after_name.len())..];
        }

        None
    }

    fn parse_svg_length(value: String) -> Option<f64> {
        let value = value.trim();
        if value.ends_with('%') {
            return None;
        }

        let number: String = value
            .chars()
            .take_while(|character| {
                character.is_ascii_digit()
                    || *character == '.'
                    || *character == '+'
                    || *character == '-'
            })
            .collect();

        let parsed = number.parse::<f64>().ok()?;
        parsed.is_finite().then_some(parsed.max(1.0))
    }

    fn parse_svg_view_box(value: String) -> Option<(f64, f64)> {
        let parts: Vec<f64> = value
            .replace(',', " ")
            .split_whitespace()
            .filter_map(|part| part.parse::<f64>().ok())
            .collect();

        if parts.len() == 4 && parts[2].is_finite() && parts[3].is_finite() {
            Some((parts[2].abs().max(1.0), parts[3].abs().max(1.0)))
        } else {
            None
        }
    }

    fn image_from_bytes(bytes: &[u8]) -> Option<objc2::rc::Retained<NSImage>> {
        let data = NSData::with_bytes(bytes);
        NSImage::initWithData(NSImage::alloc(), &data)
    }

    #[allow(deprecated)]
    fn image_preview_png_bytes(
        image: &NSImage,
        original_bytes: &[u8],
        pasteboard_type: ImagePasteboardType,
    ) -> Option<Vec<u8>> {
        if pasteboard_type == ImagePasteboardType::Png
            && original_bytes.len() <= MAX_DIRECT_IMAGE_DATA_URL_BYTES
        {
            return Some(original_bytes.to_vec());
        }

        let size = image.size();
        let target_size = thumbnail_size(size);
        let thumbnail = NSImage::initWithSize(NSImage::alloc(), target_size);
        thumbnail.lockFocus();
        image.drawInRect_fromRect_operation_fraction(
            NSRect::new(NSPoint::new(0.0, 0.0), target_size),
            NSRect::new(NSPoint::new(0.0, 0.0), size),
            NSCompositingOperation::SourceOver,
            1.0,
        );
        thumbnail.unlockFocus();

        let tiff_data = thumbnail.TIFFRepresentation()?;
        let bitmap = NSBitmapImageRep::imageRepWithData(&tiff_data)?;
        let properties =
            NSDictionary::<objc2_app_kit::NSBitmapImageRepPropertyKey, AnyObject>::new();
        let png_data = unsafe {
            bitmap.representationUsingType_properties(NSBitmapImageFileType::PNG, &properties)
        }?;

        Some(png_data.to_vec())
    }

    fn thumbnail_size(size: NSSize) -> NSSize {
        let width = size.width.max(1.0);
        let height = size.height.max(1.0);
        let scale = (IMAGE_THUMBNAIL_MAX_EDGE / width)
            .min(IMAGE_THUMBNAIL_MAX_EDGE / height)
            .min(1.0);

        NSSize::new(
            (width * scale).round().max(1.0),
            (height * scale).round().max(1.0),
        )
    }
}

#[cfg(target_os = "windows")]
mod platform {
    use super::{
        ClipboardCandidate, ClipboardFileReference, ClipboardImagePayload, ClipboardItemKind,
        ClipboardPayload, ClipboardPayloadItem, ClipboardReadSnapshot, ImageCandidateData,
        ImagePasteboardType, MAX_DIRECT_IMAGE_DATA_URL_BYTES, MAX_IMAGE_BYTES, MAX_TEXT_CHARS,
        MAX_TEXT_PREVIEW_CHARS, hash_parts, hash_payload_items, truncate_chars,
    };
    use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
    use std::mem;
    use std::path::Path;
    use std::ptr;
    use std::slice;
    use std::thread;
    use std::time::Duration;
    use windows_sys::Win32::Foundation::{GlobalFree, HGLOBAL};
    use windows_sys::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, GetClipboardData, GetClipboardSequenceNumber,
        IsClipboardFormatAvailable, OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
    };
    use windows_sys::Win32::System::Memory::{
        GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock,
    };
    use windows_sys::Win32::System::Ole::{CF_DIB, CF_DIBV5, CF_HDROP, CF_UNICODETEXT};
    use windows_sys::Win32::UI::Shell::{DROPFILES, DragQueryFileW};

    const PNG_CLIPBOARD_FORMAT: &str = "PNG";

    struct ClipboardGuard;

    impl Drop for ClipboardGuard {
        fn drop(&mut self) {
            unsafe {
                CloseClipboard();
            }
        }
    }

    pub(super) fn read_clipboard_snapshot(
        last_change_count: i64,
    ) -> Result<ClipboardReadSnapshot, String> {
        let change_count = unsafe { GetClipboardSequenceNumber() } as i64;

        if change_count == last_change_count {
            return Ok(ClipboardReadSnapshot {
                change_count,
                candidate: None,
            });
        }

        let _clipboard = open_clipboard()?;
        let candidate = read_file_items_candidate()
            .or_else(read_image_items_candidate)
            .or_else(read_text_candidate);

        Ok(ClipboardReadSnapshot {
            change_count,
            candidate,
        })
    }

    pub(super) fn write_payload_to_pasteboard(payload: ClipboardPayload) -> Result<(), String> {
        let _clipboard = open_clipboard()?;

        if (unsafe { EmptyClipboard() }) == 0 {
            return Err("Windows rejected the clipboard clear request".into());
        }

        match payload {
            ClipboardPayload::Text(text) => write_text_to_clipboard(&text),
            ClipboardPayload::Items(items) => write_items_to_clipboard(&items),
        }
    }

    fn open_clipboard() -> Result<ClipboardGuard, String> {
        for _ in 0..8 {
            if (unsafe { OpenClipboard(ptr::null_mut()) }) != 0 {
                return Ok(ClipboardGuard);
            }
            thread::sleep(Duration::from_millis(20));
        }

        Err("Windows clipboard is busy; try again in a moment".into())
    }

    fn read_text_candidate() -> Option<ClipboardCandidate> {
        let text = read_utf16_clipboard(u32::from(CF_UNICODETEXT))?;
        if text.trim().is_empty() {
            return None;
        }

        let bytes = text.as_bytes();
        let char_count = text.chars().count();
        let line_count = text.lines().count().max(1);
        let stored_text = truncate_chars(&text, MAX_TEXT_CHARS);
        let preview_text = truncate_chars(&text, MAX_TEXT_PREVIEW_CHARS);

        Some(ClipboardCandidate {
            kind: ClipboardItemKind::Text,
            preview_text: Some(preview_text),
            image_data_url: None,
            files: Vec::new(),
            item_count: 1,
            byte_count: bytes.len(),
            char_count: Some(char_count),
            line_count: Some(line_count),
            width: None,
            height: None,
            source_label: Some("TEXT".into()),
            source_path: None,
            hash: hash_parts(ClipboardItemKind::Text, bytes),
            payload: ClipboardPayload::Text(stored_text),
        })
    }

    fn read_file_items_candidate() -> Option<ClipboardCandidate> {
        let paths = file_paths_from_clipboard();
        if paths.is_empty() {
            return None;
        }

        let mut files = Vec::with_capacity(paths.len());
        let mut payload_items = Vec::with_capacity(paths.len());
        let mut total_bytes: u64 = 0;
        let mut image_count = 0;
        let mut first_preview = None;
        let mut width = None;
        let mut height = None;

        for path in &paths {
            let image_data = image_candidate_from_file_path(path);
            let file = file_reference_from_path(path, image_data.as_ref());

            if file.is_image {
                image_count += 1;
                if first_preview.is_none() {
                    first_preview = image_data
                        .as_ref()
                        .and_then(|image| image.summary_preview.clone());
                    width = image_data.as_ref().and_then(|image| image.width);
                    height = image_data.as_ref().and_then(|image| image.height);
                }
            }

            if let Some(byte_count) = file.byte_count {
                total_bytes = total_bytes.saturating_add(byte_count);
            }

            payload_items.push(ClipboardPayloadItem {
                file_path: Some(path.clone()),
                image: image_data.map(|image| image.payload),
            });
            files.push(file);
        }

        let kind = if paths.len() == 1 {
            if files.first().map(|file| file.is_image).unwrap_or(false) {
                ClipboardItemKind::Image
            } else {
                ClipboardItemKind::File
            }
        } else {
            ClipboardItemKind::Group
        };
        let source_label = file_source_label(&files, image_count);
        let preview_text = file_preview_text(&files);
        let byte_count = usize::try_from(total_bytes).unwrap_or(usize::MAX);
        let hash = hash_payload_items(kind, &payload_items);

        Some(ClipboardCandidate {
            kind,
            preview_text,
            image_data_url: first_preview,
            files,
            item_count: payload_items.len(),
            byte_count,
            char_count: None,
            line_count: None,
            width,
            height,
            source_label: Some(source_label),
            source_path: paths.first().cloned(),
            hash,
            payload: ClipboardPayload::Items(payload_items),
        })
    }

    fn read_image_items_candidate() -> Option<ClipboardCandidate> {
        let image = image_candidate_from_clipboard()?;
        let byte_count = image.byte_count;
        let image_data_url = image.summary_preview.clone();
        let width = image.width;
        let height = image.height;
        let source_label = image.type_label.to_string();
        let payload_items = vec![ClipboardPayloadItem {
            file_path: None,
            image: Some(image.payload),
        }];
        let hash = hash_payload_items(ClipboardItemKind::Image, &payload_items);

        Some(ClipboardCandidate {
            kind: ClipboardItemKind::Image,
            preview_text: None,
            image_data_url,
            files: Vec::new(),
            item_count: 1,
            byte_count,
            char_count: None,
            line_count: None,
            width,
            height,
            source_label: Some(source_label),
            source_path: None,
            hash,
            payload: ClipboardPayload::Items(payload_items),
        })
    }

    fn file_paths_from_clipboard() -> Vec<String> {
        if !format_available(u32::from(CF_HDROP)) {
            return Vec::new();
        }

        let handle = unsafe { GetClipboardData(u32::from(CF_HDROP)) };
        if handle.is_null() {
            return Vec::new();
        }

        let count = unsafe { DragQueryFileW(handle, u32::MAX, ptr::null_mut(), 0) };
        let mut paths = Vec::with_capacity(count as usize);

        for index in 0..count {
            let required_chars = unsafe { DragQueryFileW(handle, index, ptr::null_mut(), 0) };
            if required_chars == 0 {
                continue;
            }

            let mut buffer = vec![0u16; required_chars as usize + 1];
            let written =
                unsafe { DragQueryFileW(handle, index, buffer.as_mut_ptr(), buffer.len() as u32) };
            if written == 0 {
                continue;
            }

            let path = String::from_utf16_lossy(&buffer[..written as usize]);
            if !paths.iter().any(|existing| existing == &path) {
                paths.push(path);
            }
        }

        paths
    }

    fn image_candidate_from_clipboard() -> Option<ImageCandidateData> {
        let png_format = register_clipboard_format(PNG_CLIPBOARD_FORMAT);
        if png_format != 0 {
            if let Some(bytes) = read_clipboard_bytes(png_format) {
                if let Some(candidate) = image_candidate_from_data(ImagePasteboardType::Png, bytes)
                {
                    return Some(candidate);
                }
            }
        }

        read_clipboard_bytes(u32::from(CF_DIBV5))
            .and_then(image_candidate_from_dib)
            .or_else(|| read_clipboard_bytes(u32::from(CF_DIB)).and_then(image_candidate_from_dib))
    }

    fn image_candidate_from_dib(dib: Vec<u8>) -> Option<ImageCandidateData> {
        let bmp = bmp_file_from_dib(&dib)?;
        image_candidate_from_data(ImagePasteboardType::Bmp, bmp)
    }

    fn image_candidate_from_file_path(path: &str) -> Option<ImageCandidateData> {
        let pasteboard_type = image_pasteboard_type_from_path(path)?;
        let bytes = std::fs::read(path).ok()?;
        image_candidate_from_data(pasteboard_type, bytes)
    }

    fn image_candidate_from_data(
        pasteboard_type: ImagePasteboardType,
        bytes: Vec<u8>,
    ) -> Option<ImageCandidateData> {
        if bytes.is_empty() || bytes.len() > MAX_IMAGE_BYTES {
            return None;
        }

        let (width, height) = image_dimensions_from_bytes(pasteboard_type, &bytes);
        let summary_preview = (bytes.len() <= MAX_DIRECT_IMAGE_DATA_URL_BYTES)
            .then(|| data_url(mime_type_for_image(pasteboard_type), &bytes));
        let byte_count = bytes.len();

        Some(ImageCandidateData {
            summary_preview: summary_preview.clone(),
            file_preview: summary_preview,
            width,
            height,
            payload: ClipboardImagePayload {
                bytes,
                pasteboard_type,
            },
            byte_count,
            type_label: image_type_label(pasteboard_type),
        })
    }

    fn file_reference_from_path(
        path: &str,
        image_data: Option<&ImageCandidateData>,
    ) -> ClipboardFileReference {
        let path_ref = Path::new(path);
        let metadata = std::fs::metadata(path).ok();
        let is_directory = metadata
            .as_ref()
            .map(|metadata| metadata.is_dir())
            .unwrap_or(false);
        let is_image = !is_directory && image_pasteboard_type_from_path(path).is_some();
        let name = path_ref
            .file_name()
            .map(|value| value.to_string_lossy().into_owned())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| path.to_string());
        let type_label = file_type_label(path_ref, is_directory);

        ClipboardFileReference {
            path: path.to_string(),
            name,
            type_label,
            byte_count: metadata
                .as_ref()
                .and_then(|metadata| (!metadata.is_dir()).then_some(metadata.len())),
            is_directory,
            is_image,
            image_data_url: image_data.and_then(|image| image.file_preview.clone()),
            width: image_data.and_then(|image| image.width),
            height: image_data.and_then(|image| image.height),
        }
    }

    fn write_text_to_clipboard(text: &str) -> Result<(), String> {
        let mut wide = text.encode_utf16().collect::<Vec<_>>();
        wide.push(0);
        write_clipboard_bytes(u32::from(CF_UNICODETEXT), u16_slice_as_bytes(&wide))
    }

    fn write_items_to_clipboard(items: &[ClipboardPayloadItem]) -> Result<(), String> {
        let file_paths = items
            .iter()
            .filter_map(|item| item.file_path.as_deref())
            .collect::<Vec<_>>();
        let image = items.iter().find_map(|item| item.image.as_ref());
        let mut wrote_any = false;
        let mut errors = Vec::new();

        if !file_paths.is_empty() {
            match write_file_list_to_clipboard(&file_paths) {
                Ok(()) => wrote_any = true,
                Err(error) => errors.push(error),
            }
        }

        if let Some(image) = image {
            match write_image_to_clipboard(image) {
                Ok(()) => wrote_any = true,
                Err(error) => errors.push(error),
            }
        }

        if wrote_any {
            Ok(())
        } else if errors.is_empty() {
            Err("Clipboard item has no writable Windows payload".into())
        } else {
            Err(errors.join("; "))
        }
    }

    fn write_file_list_to_clipboard(paths: &[&str]) -> Result<(), String> {
        let wide_paths = paths
            .iter()
            .map(|path| {
                let mut wide = path.encode_utf16().collect::<Vec<_>>();
                wide.push(0);
                wide
            })
            .collect::<Vec<_>>();

        let path_bytes = wide_paths
            .iter()
            .fold(mem::size_of::<u16>(), |total, path| {
                total + path.len() * mem::size_of::<u16>()
            });
        let header_size = mem::size_of::<DROPFILES>();
        let total_size = header_size + path_bytes;
        let handle = allocate_global(total_size)?;

        unsafe {
            let pointer = GlobalLock(handle) as *mut u8;
            if pointer.is_null() {
                GlobalFree(handle);
                return Err("Failed to lock Windows file-list clipboard memory".into());
            }

            let dropfiles = DROPFILES {
                pFiles: header_size as u32,
                fWide: 1,
                ..Default::default()
            };
            ptr::write(pointer as *mut DROPFILES, dropfiles);

            let mut cursor = pointer.add(header_size) as *mut u16;
            for path in &wide_paths {
                ptr::copy_nonoverlapping(path.as_ptr(), cursor, path.len());
                cursor = cursor.add(path.len());
            }
            cursor.write(0);
            GlobalUnlock(handle);
        }

        set_clipboard_handle(u32::from(CF_HDROP), handle)
    }

    fn write_image_to_clipboard(image: &ClipboardImagePayload) -> Result<(), String> {
        let mut wrote_any = false;
        let mut errors = Vec::new();

        match dib_clipboard_bytes_from_image(image) {
            Some(dib) => match write_clipboard_bytes(u32::from(CF_DIB), &dib) {
                Ok(()) => wrote_any = true,
                Err(error) => errors.push(error),
            },
            None => errors.push(format!(
                "{} image data could not be converted to a Windows bitmap clipboard format",
                image_type_label(image.pasteboard_type)
            )),
        }

        if should_publish_raw_image_format(image.pasteboard_type) {
            match write_raw_image_format_to_clipboard(image) {
                Ok(()) => wrote_any = true,
                Err(error) => errors.push(error),
            }
        }

        if wrote_any {
            Ok(())
        } else {
            Err(errors.join("; "))
        }
    }

    fn write_raw_image_format_to_clipboard(image: &ClipboardImagePayload) -> Result<(), String> {
        let format_name = match image.pasteboard_type {
            ImagePasteboardType::Png => PNG_CLIPBOARD_FORMAT,
            other => image_type_label(other),
        };
        let format = register_clipboard_format(format_name);
        if format == 0 {
            return Err(format!(
                "Windows did not register the {format_name} clipboard format"
            ));
        }
        write_clipboard_bytes(format, &image.bytes)
    }

    fn should_publish_raw_image_format(pasteboard_type: ImagePasteboardType) -> bool {
        !matches!(pasteboard_type, ImagePasteboardType::Bmp)
    }

    fn write_clipboard_bytes(format: u32, bytes: &[u8]) -> Result<(), String> {
        let handle = allocate_global_from_bytes(bytes)?;
        set_clipboard_handle(format, handle)
    }

    fn set_clipboard_handle(format: u32, handle: HGLOBAL) -> Result<(), String> {
        if (unsafe { SetClipboardData(format, handle) }).is_null() {
            unsafe {
                GlobalFree(handle);
            }
            return Err("Windows rejected the clipboard data".into());
        }

        Ok(())
    }

    fn allocate_global_from_bytes(bytes: &[u8]) -> Result<HGLOBAL, String> {
        let handle = allocate_global(bytes.len().max(1))?;
        unsafe {
            let pointer = GlobalLock(handle) as *mut u8;
            if pointer.is_null() {
                GlobalFree(handle);
                return Err("Failed to lock Windows clipboard memory".into());
            }
            ptr::copy_nonoverlapping(bytes.as_ptr(), pointer, bytes.len());
            GlobalUnlock(handle);
        }
        Ok(handle)
    }

    fn allocate_global(size: usize) -> Result<HGLOBAL, String> {
        let handle = unsafe { GlobalAlloc(GMEM_MOVEABLE, size) };
        if handle.is_null() {
            Err("Failed to allocate Windows clipboard memory".into())
        } else {
            Ok(handle)
        }
    }

    fn read_utf16_clipboard(format: u32) -> Option<String> {
        let bytes = read_clipboard_bytes(format)?;
        let mut units = bytes
            .chunks_exact(2)
            .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
            .collect::<Vec<_>>();
        let end = units
            .iter()
            .position(|unit| *unit == 0)
            .unwrap_or(units.len());
        units.truncate(end);
        String::from_utf16(&units).ok()
    }

    fn read_clipboard_bytes(format: u32) -> Option<Vec<u8>> {
        if !format_available(format) {
            return None;
        }

        let handle = unsafe { GetClipboardData(format) };
        if handle.is_null() {
            return None;
        }

        let size = unsafe { GlobalSize(handle) };
        if size == 0 || size > MAX_IMAGE_BYTES.max(MAX_TEXT_CHARS * 4) {
            return None;
        }

        let pointer = unsafe { GlobalLock(handle) } as *const u8;
        if pointer.is_null() {
            return None;
        }

        let bytes = unsafe { slice::from_raw_parts(pointer, size) }.to_vec();
        unsafe {
            GlobalUnlock(handle);
        }
        Some(bytes)
    }

    fn format_available(format: u32) -> bool {
        (unsafe { IsClipboardFormatAvailable(format) }) != 0
    }

    fn register_clipboard_format(name: &str) -> u32 {
        let mut wide = name.encode_utf16().collect::<Vec<_>>();
        wide.push(0);
        unsafe { RegisterClipboardFormatW(wide.as_ptr()) }
    }

    fn u16_slice_as_bytes(values: &[u16]) -> &[u8] {
        unsafe { slice::from_raw_parts(values.as_ptr() as *const u8, mem::size_of_val(values)) }
    }

    fn data_url(mime_type: &str, bytes: &[u8]) -> String {
        format!("data:{mime_type};base64,{}", BASE64_STANDARD.encode(bytes))
    }

    fn bmp_file_from_dib(dib: &[u8]) -> Option<Vec<u8>> {
        if dib.len() < 16 {
            return None;
        }

        let dib_offset = dib_pixel_offset(dib)?;
        let file_size = 14usize.checked_add(dib.len())?;
        let file_size_u32 = u32::try_from(file_size).ok()?;
        let pixel_offset = 14u32.checked_add(dib_offset)?;
        let mut bmp = Vec::with_capacity(file_size);

        bmp.extend_from_slice(b"BM");
        bmp.extend_from_slice(&file_size_u32.to_le_bytes());
        bmp.extend_from_slice(&0u16.to_le_bytes());
        bmp.extend_from_slice(&0u16.to_le_bytes());
        bmp.extend_from_slice(&pixel_offset.to_le_bytes());
        bmp.extend_from_slice(dib);

        Some(bmp)
    }

    fn dib_bytes_from_bmp_or_dib(bytes: &[u8]) -> &[u8] {
        if bytes.len() > 14 && bytes.starts_with(b"BM") {
            &bytes[14..]
        } else {
            bytes
        }
    }

    fn dib_clipboard_bytes_from_image(image: &ClipboardImagePayload) -> Option<Vec<u8>> {
        match image.pasteboard_type {
            ImagePasteboardType::Bmp => Some(dib_bytes_from_bmp_or_dib(&image.bytes).to_vec()),
            ImagePasteboardType::Heic | ImagePasteboardType::Heif | ImagePasteboardType::Svg => {
                None
            }
            ImagePasteboardType::Png
            | ImagePasteboardType::Tiff
            | ImagePasteboardType::Jpeg
            | ImagePasteboardType::Gif
            | ImagePasteboardType::Webp => decoded_image_to_dib_bytes(&image.bytes),
        }
    }

    fn decoded_image_to_dib_bytes(bytes: &[u8]) -> Option<Vec<u8>> {
        let image = image::load_from_memory(bytes).ok()?.to_rgba8();
        let width = image.width();
        let height = image.height();
        let width_i32 = i32::try_from(width).ok()?;
        let top_down_height = i32::try_from(height).ok()?.checked_neg()?;
        let pixel_byte_count = usize::try_from(width)
            .ok()?
            .checked_mul(usize::try_from(height).ok()?)?
            .checked_mul(4)?;
        let pixel_byte_count_u32 = u32::try_from(pixel_byte_count).ok()?;

        let mut dib = Vec::with_capacity(40usize.checked_add(pixel_byte_count)?);
        dib.extend_from_slice(&40u32.to_le_bytes());
        dib.extend_from_slice(&width_i32.to_le_bytes());
        dib.extend_from_slice(&top_down_height.to_le_bytes());
        dib.extend_from_slice(&1u16.to_le_bytes());
        dib.extend_from_slice(&32u16.to_le_bytes());
        dib.extend_from_slice(&0u32.to_le_bytes());
        dib.extend_from_slice(&pixel_byte_count_u32.to_le_bytes());
        dib.extend_from_slice(&0i32.to_le_bytes());
        dib.extend_from_slice(&0i32.to_le_bytes());
        dib.extend_from_slice(&0u32.to_le_bytes());
        dib.extend_from_slice(&0u32.to_le_bytes());

        for pixel in image.pixels() {
            let [red, green, blue, alpha] = pixel.0;
            dib.extend_from_slice(&[blue, green, red, alpha]);
        }

        Some(dib)
    }

    fn dib_pixel_offset(dib: &[u8]) -> Option<u32> {
        let header_size = u32_from_le(dib, 0)?;
        let bit_count = u16_from_le(dib, 14).unwrap_or(32);
        let compression = u32_from_le(dib, 16).unwrap_or(0);
        let colors_used = u32_from_le(dib, 32).unwrap_or(0);
        let palette_colors = if colors_used > 0 {
            colors_used
        } else if bit_count <= 8 {
            1u32.checked_shl(u32::from(bit_count)).unwrap_or(0)
        } else {
            0
        };
        let masks_size = if compression == 3 && header_size == 40 {
            12
        } else if compression == 6 && header_size == 40 {
            16
        } else {
            0
        };

        header_size
            .checked_add(masks_size)
            .and_then(|value| value.checked_add(palette_colors.checked_mul(4)?))
    }

    fn image_dimensions_from_bytes(
        pasteboard_type: ImagePasteboardType,
        bytes: &[u8],
    ) -> (Option<f64>, Option<f64>) {
        let dimensions = match pasteboard_type {
            ImagePasteboardType::Png => png_dimensions(bytes),
            ImagePasteboardType::Jpeg => jpeg_dimensions(bytes),
            ImagePasteboardType::Gif => gif_dimensions(bytes),
            ImagePasteboardType::Webp => webp_dimensions(bytes),
            ImagePasteboardType::Bmp => bmp_dimensions(bytes),
            ImagePasteboardType::Svg => svg_dimensions_from_bytes(bytes),
            ImagePasteboardType::Tiff | ImagePasteboardType::Heic | ImagePasteboardType::Heif => {
                None
            }
        };

        dimensions
            .map(|(width, height)| (Some(width), Some(height)))
            .unwrap_or((None, None))
    }

    fn png_dimensions(bytes: &[u8]) -> Option<(f64, f64)> {
        if bytes.len() < 24 || !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
            return None;
        }

        Some((
            u32::from_be_bytes(bytes[16..20].try_into().ok()?) as f64,
            u32::from_be_bytes(bytes[20..24].try_into().ok()?) as f64,
        ))
    }

    fn jpeg_dimensions(bytes: &[u8]) -> Option<(f64, f64)> {
        if bytes.len() < 4 || bytes[0] != 0xFF || bytes[1] != 0xD8 {
            return None;
        }

        let mut index = 2;
        while index + 9 < bytes.len() {
            while index < bytes.len() && bytes[index] != 0xFF {
                index += 1;
            }
            while index < bytes.len() && bytes[index] == 0xFF {
                index += 1;
            }
            if index >= bytes.len() {
                return None;
            }

            let marker = bytes[index];
            index += 1;
            if marker == 0xD9 || marker == 0xDA {
                return None;
            }
            if index + 2 > bytes.len() {
                return None;
            }

            let segment_len = u16::from_be_bytes([bytes[index], bytes[index + 1]]) as usize;
            if segment_len < 2 || index + segment_len > bytes.len() {
                return None;
            }

            if matches!(
                marker,
                0xC0 | 0xC1
                    | 0xC2
                    | 0xC3
                    | 0xC5
                    | 0xC6
                    | 0xC7
                    | 0xC9
                    | 0xCA
                    | 0xCB
                    | 0xCD
                    | 0xCE
                    | 0xCF
            ) && segment_len >= 7
            {
                let height = u16::from_be_bytes([bytes[index + 3], bytes[index + 4]]) as f64;
                let width = u16::from_be_bytes([bytes[index + 5], bytes[index + 6]]) as f64;
                return Some((width, height));
            }

            index += segment_len;
        }

        None
    }

    fn gif_dimensions(bytes: &[u8]) -> Option<(f64, f64)> {
        if bytes.len() < 10 || (!bytes.starts_with(b"GIF87a") && !bytes.starts_with(b"GIF89a")) {
            return None;
        }

        Some((
            u16::from_le_bytes(bytes[6..8].try_into().ok()?) as f64,
            u16::from_le_bytes(bytes[8..10].try_into().ok()?) as f64,
        ))
    }

    fn webp_dimensions(bytes: &[u8]) -> Option<(f64, f64)> {
        if bytes.len() < 30 || !bytes.starts_with(b"RIFF") || &bytes[8..12] != b"WEBP" {
            return None;
        }

        match &bytes[12..16] {
            b"VP8X" if bytes.len() >= 30 => {
                let width = 1
                    + u32::from(bytes[24])
                    + (u32::from(bytes[25]) << 8)
                    + (u32::from(bytes[26]) << 16);
                let height = 1
                    + u32::from(bytes[27])
                    + (u32::from(bytes[28]) << 8)
                    + (u32::from(bytes[29]) << 16);
                Some((width as f64, height as f64))
            }
            b"VP8 " if bytes.len() >= 30 => {
                let width = u16::from_le_bytes([bytes[26], bytes[27]]) & 0x3FFF;
                let height = u16::from_le_bytes([bytes[28], bytes[29]]) & 0x3FFF;
                Some((width as f64, height as f64))
            }
            b"VP8L" if bytes.len() >= 25 => {
                let bits = u32::from(bytes[21])
                    | (u32::from(bytes[22]) << 8)
                    | (u32::from(bytes[23]) << 16)
                    | (u32::from(bytes[24]) << 24);
                let width = (bits & 0x3FFF) + 1;
                let height = ((bits >> 14) & 0x3FFF) + 1;
                Some((width as f64, height as f64))
            }
            _ => None,
        }
    }

    fn bmp_dimensions(bytes: &[u8]) -> Option<(f64, f64)> {
        let offset = if bytes.starts_with(b"BM") { 14 } else { 0 };
        let width = i32_from_le(bytes, offset + 4)?.unsigned_abs().max(1) as f64;
        let height = i32_from_le(bytes, offset + 8)?.unsigned_abs().max(1) as f64;
        Some((width, height))
    }

    fn svg_dimensions_from_bytes(bytes: &[u8]) -> Option<(f64, f64)> {
        let text = std::str::from_utf8(bytes).ok()?;
        let svg_tag = svg_opening_tag(text)?;
        let width = svg_attribute(&svg_tag, "width").and_then(parse_svg_length);
        let height = svg_attribute(&svg_tag, "height").and_then(parse_svg_length);
        if let (Some(width), Some(height)) = (width, height) {
            return Some((width, height));
        }

        let view_box = svg_attribute(&svg_tag, "viewBox")
            .or_else(|| svg_attribute(&svg_tag, "viewbox"))
            .and_then(parse_svg_view_box);
        match (width, height, view_box) {
            (Some(width), None, Some((_, view_box_height))) => Some((width, view_box_height)),
            (None, Some(height), Some((view_box_width, _))) => Some((view_box_width, height)),
            (None, None, Some(dimensions)) => Some(dimensions),
            _ => None,
        }
    }

    fn svg_opening_tag(text: &str) -> Option<String> {
        let start = text.find("<svg")?;
        let end = text[start..].find('>')?;
        Some(text[start..start + end].to_string())
    }

    fn svg_attribute(tag: &str, name: &str) -> Option<String> {
        let mut remaining = tag;
        while let Some(offset) = remaining.find(name) {
            let name_start = tag.len() - remaining.len() + offset;
            let before_name = tag[..name_start].chars().next_back();
            let after_name = &tag[name_start + name.len()..];

            let starts_attribute = before_name
                .map(|character| character.is_whitespace() || character == '<')
                .unwrap_or(false);
            if starts_attribute && after_name.trim_start().starts_with('=') {
                let after_equals = after_name.trim_start().strip_prefix('=')?.trim_start();
                let quote = after_equals.chars().next()?;
                if quote != '"' && quote != '\'' {
                    return None;
                }
                let value_start = quote.len_utf8();
                let value_end = after_equals[value_start..].find(quote)?;
                return Some(after_equals[value_start..value_start + value_end].to_string());
            }

            remaining = &after_name[after_name
                .char_indices()
                .nth(1)
                .map(|(index, _)| index)
                .unwrap_or(after_name.len())..];
        }

        None
    }

    fn parse_svg_length(value: String) -> Option<f64> {
        let value = value.trim();
        if value.ends_with('%') {
            return None;
        }

        let number: String = value
            .chars()
            .take_while(|character| {
                character.is_ascii_digit()
                    || *character == '.'
                    || *character == '+'
                    || *character == '-'
            })
            .collect();

        let parsed = number.parse::<f64>().ok()?;
        parsed.is_finite().then_some(parsed.max(1.0))
    }

    fn parse_svg_view_box(value: String) -> Option<(f64, f64)> {
        let parts: Vec<f64> = value
            .replace(',', " ")
            .split_whitespace()
            .filter_map(|part| part.parse::<f64>().ok())
            .collect();

        if parts.len() == 4 && parts[2].is_finite() && parts[3].is_finite() {
            Some((parts[2].abs().max(1.0), parts[3].abs().max(1.0)))
        } else {
            None
        }
    }

    fn image_pasteboard_type_from_path(path: &str) -> Option<ImagePasteboardType> {
        let extension = path.rsplit('.').next()?.to_ascii_lowercase();
        match extension.as_str() {
            "png" => Some(ImagePasteboardType::Png),
            "jpg" | "jpeg" => Some(ImagePasteboardType::Jpeg),
            "tif" | "tiff" => Some(ImagePasteboardType::Tiff),
            "heic" => Some(ImagePasteboardType::Heic),
            "heif" => Some(ImagePasteboardType::Heif),
            "gif" => Some(ImagePasteboardType::Gif),
            "webp" => Some(ImagePasteboardType::Webp),
            "bmp" => Some(ImagePasteboardType::Bmp),
            "svg" => Some(ImagePasteboardType::Svg),
            _ => None,
        }
    }

    fn image_type_label(pasteboard_type: ImagePasteboardType) -> &'static str {
        match pasteboard_type {
            ImagePasteboardType::Png => "PNG",
            ImagePasteboardType::Tiff => "TIFF",
            ImagePasteboardType::Jpeg => "JPEG",
            ImagePasteboardType::Heic => "HEIC",
            ImagePasteboardType::Heif => "HEIF",
            ImagePasteboardType::Gif => "GIF",
            ImagePasteboardType::Webp => "WEBP",
            ImagePasteboardType::Bmp => "BMP",
            ImagePasteboardType::Svg => "SVG",
        }
    }

    fn mime_type_for_image(pasteboard_type: ImagePasteboardType) -> &'static str {
        match pasteboard_type {
            ImagePasteboardType::Png => "image/png",
            ImagePasteboardType::Tiff => "image/tiff",
            ImagePasteboardType::Jpeg => "image/jpeg",
            ImagePasteboardType::Heic => "image/heic",
            ImagePasteboardType::Heif => "image/heif",
            ImagePasteboardType::Gif => "image/gif",
            ImagePasteboardType::Webp => "image/webp",
            ImagePasteboardType::Bmp => "image/bmp",
            ImagePasteboardType::Svg => "image/svg+xml",
        }
    }

    fn file_type_label(path: &Path, is_directory: bool) -> String {
        if let Some(extension) = path.extension().and_then(|value| value.to_str()) {
            let extension = extension.trim();
            if !extension.is_empty() {
                return extension.to_ascii_uppercase();
            }
        }

        if is_directory {
            "FOLDER".into()
        } else {
            "FILE".into()
        }
    }

    fn file_source_label(files: &[ClipboardFileReference], image_count: usize) -> String {
        match files {
            [single] if single.is_directory => "FOLDER".into(),
            [single] => single.type_label.clone(),
            _ if image_count == files.len() => format!("{} IMAGES", files.len()),
            _ => format!("{} FILES", files.len()),
        }
    }

    fn file_preview_text(files: &[ClipboardFileReference]) -> Option<String> {
        if files.is_empty() {
            return None;
        }

        let mut names = files
            .iter()
            .take(4)
            .map(|file| file.name.as_str())
            .collect::<Vec<_>>()
            .join("\n");

        if files.len() > 4 {
            names.push_str(&format!("\n+{} more", files.len() - 4));
        }

        Some(names)
    }

    fn u16_from_le(bytes: &[u8], offset: usize) -> Option<u16> {
        Some(u16::from_le_bytes(
            bytes.get(offset..offset + 2)?.try_into().ok()?,
        ))
    }

    fn u32_from_le(bytes: &[u8], offset: usize) -> Option<u32> {
        Some(u32::from_le_bytes(
            bytes.get(offset..offset + 4)?.try_into().ok()?,
        ))
    }

    fn i32_from_le(bytes: &[u8], offset: usize) -> Option<i32> {
        Some(i32::from_le_bytes(
            bytes.get(offset..offset + 4)?.try_into().ok()?,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::{ClipboardItemKind, hash_parts, truncate_chars};

    #[test]
    fn truncates_text_preview_by_character_count() {
        assert_eq!(truncate_chars("abcdef", 3), "abc…");
        assert_eq!(truncate_chars("你好世界", 3), "你好世…");
        assert_eq!(truncate_chars("abc", 3), "abc");
    }

    #[test]
    fn hashes_include_the_clipboard_kind() {
        assert_ne!(
            hash_parts(ClipboardItemKind::Text, b"same"),
            hash_parts(ClipboardItemKind::Image, b"same"),
        );
    }

    #[test]
    fn public_item_shape_keeps_expected_clipboard_fields() {
        let item = super::ClipboardHistoryItem {
            id: "clip-1".into(),
            kind: ClipboardItemKind::Text,
            preview_text: Some("hello".into()),
            image_data_url: None,
            files: Vec::new(),
            item_count: 1,
            byte_count: 5,
            char_count: Some(5),
            line_count: Some(1),
            width: None,
            height: None,
            created_at: 1,
            is_pinned: false,
            source_label: Some("TEXT".into()),
            source_path: None,
            hash: "abc".into(),
        };

        assert_eq!(item.id, "clip-1");
        assert_eq!(item.preview_text.as_deref(), Some("hello"));
    }
}
