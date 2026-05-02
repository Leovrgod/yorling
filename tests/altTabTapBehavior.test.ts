import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

test('defers the custom Alt+Tab overlay for a single tap', () => {
  const source = readFileSync('src-tauri/src/alt_tab.rs', 'utf8');

  assert.match(source, /const ALT_TAB_OVERLAY_REVEAL_DELAY_MS: u64 = 220;/);
  assert.match(
    source,
    /CycleUpdate::DeferredReveal\s*\{\s*session_id: session\.session_id\(\),\s*\}/s,
  );
  assert.match(
    source,
    /thread::sleep\(Duration::from_millis\(ALT_TAB_OVERLAY_REVEAL_DELAY_MS\)\);\s*let _ = command_sender\.send\(AltTabCommand::RevealPendingSession \{ session_id \}\);/s,
  );
  assert.match(
    source,
    /else if overlay_was_visible\s*\{\s*CycleUpdate::Selection\(session\.overlay_selection\(\)\)\s*\}\s*else\s*\{\s*CycleUpdate::Full\(session\.overlay_state\(\)\)\s*\}/s,
  );
});
