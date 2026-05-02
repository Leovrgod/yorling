import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

test('registers the clipboard module between super right click and island', () => {
  const appSource = readFileSync('src/App.tsx', 'utf8');
  const sidebarSource = readFileSync('src/components/common/Sidebar.tsx', 'utf8');
  const typesSource = readFileSync('src/types/index.ts', 'utf8');
  const copySource = readFileSync('src/i18n/copy.ts', 'utf8');

  assert.strictEqual(typesSource.includes("'clipboard'"), true);
  assert.strictEqual(copySource.includes("clipboard: '剪贴板'"), true);
  assert.strictEqual(copySource.includes("clipboard: 'Clipboard'"), true);
  assert.ok(
    sidebarSource.indexOf("{ id: 'superRightClick', icon: '⌁' }")
      < sidebarSource.indexOf("{ id: 'clipboard', icon: '▥' }"),
  );
  assert.ok(
    sidebarSource.indexOf("{ id: 'clipboard', icon: '▥' }")
      < sidebarSource.indexOf("{ id: 'island', icon: '◆' }"),
  );
  assert.strictEqual(appSource.includes("case 'clipboard':"), true);
  assert.strictEqual(appSource.includes('<ClipboardWall />'), true);
});

test('implements clipboard history with native pasteboard polling and a note wall UI', () => {
  const commandSource = readFileSync('src-tauri/src/commands/clipboard.rs', 'utf8');
  const commandsModSource = readFileSync('src-tauri/src/commands/mod.rs', 'utf8');
  const libSource = readFileSync('src-tauri/src/lib.rs', 'utf8');
  const componentSource = readFileSync('src/components/clipboard/ClipboardWall.tsx', 'utf8');
  const styleSource = readFileSync('src/styles/nothing/components.css', 'utf8');

  assert.strictEqual(commandsModSource.includes('pub mod clipboard;'), true);
  assert.strictEqual(libSource.includes('ClipboardState::new()'), true);
  assert.strictEqual(libSource.includes('commands::clipboard::start_clipboard_monitor'), true);
  assert.strictEqual(libSource.includes('commands::clipboard::get_clipboard_history'), true);
  assert.strictEqual(libSource.includes('commands::clipboard::copy_clipboard_history_item'), true);
  assert.strictEqual(commandSource.includes('NSPasteboard::generalPasteboard()'), true);
  assert.strictEqual(commandSource.includes('changeCount()'), true);
  assert.strictEqual(commandSource.includes('org.nspasteboard.TransientType'), true);
  assert.strictEqual(commandSource.includes('com.agilebits.onepassword'), true);
  assert.strictEqual(commandSource.includes('NSPasteboardTypePNG'), true);
  assert.strictEqual(commandSource.includes('NSPasteboardTypeTIFF'), true);
  assert.strictEqual(commandSource.includes('MAX_DIRECT_IMAGE_DATA_URL_BYTES'), true);
  assert.strictEqual(commandSource.includes('IMAGE_THUMBNAIL_MAX_EDGE'), true);
  assert.strictEqual(commandSource.includes('CLIPBOARD_HISTORY_LIMIT: usize = 100'), true);
  assert.strictEqual(commandSource.includes('PersistedClipboardHistory'), true);
  assert.strictEqual(commandSource.includes('clipboard-history.json'), true);
  assert.strictEqual(commandSource.includes('source_path'), true);
  assert.strictEqual(commandSource.includes('NSPasteboardTypeFileURL'), true);
  assert.strictEqual(commandSource.includes('pasteboardItems()'), true);
  assert.strictEqual(commandSource.includes('URLWithDataRepresentation_relativeToURL'), true);
  assert.strictEqual(commandSource.includes('to_file_path()'), true);
  assert.ok(
    commandSource.indexOf('read_file_items_candidate(&pasteboard)')
      < commandSource.indexOf('read_image_items_candidate(&pasteboard)'),
  );
  assert.strictEqual(commandSource.includes('image/svg+xml'), true);
  assert.strictEqual(commandSource.includes('public.svg-image'), true);
  assert.strictEqual(commandSource.includes('ClipboardPayload::Items'), true);
  assert.strictEqual(commandSource.includes('delete_clipboard_history_item'), true);
  assert.strictEqual(commandSource.includes('set_clipboard_history_item_pinned'), true);
  assert.strictEqual(commandSource.includes('open_clipboard_image_location'), true);
  assert.strictEqual(commandSource.includes('open_clipboard_item_location'), true);
  assert.strictEqual(libSource.includes('commands::clipboard::open_clipboard_image_location'), true);
  assert.strictEqual(libSource.includes('commands::clipboard::open_clipboard_item_location'), true);
  assert.strictEqual(componentSource.includes('clipboard-note-wall'), true);
  assert.strictEqual(componentSource.includes('clipboard-search'), true);
  assert.strictEqual(componentSource.includes('delete_clipboard_history_item'), true);
  assert.strictEqual(componentSource.includes('set_clipboard_history_item_pinned'), true);
  assert.strictEqual(componentSource.includes('open_clipboard_item_location'), true);
  assert.strictEqual(componentSource.includes('clipboard-note--tone-'), true);
  assert.strictEqual(componentSource.includes('MAX_VISIBLE_CLIPBOARD_ITEMS = 100'), true);
  assert.strictEqual(componentSource.includes('getNoteTone(item, index)'), false);
  assert.strictEqual(componentSource.includes('clipboard-context-menu'), true);
  assert.strictEqual(componentSource.includes('open_clipboard_item_location'), true);
  assert.strictEqual(componentSource.includes('copy_clipboard_history_item'), true);
  assert.strictEqual(styleSource.includes('.clipboard-note-wall'), true);
  assert.strictEqual(styleSource.includes('grid-template-columns: repeat(auto-fill'), true);
  assert.strictEqual(styleSource.includes('.clipboard-note__tape'), true);
  assert.strictEqual(styleSource.includes('.clipboard-note--tone-5'), true);
});
