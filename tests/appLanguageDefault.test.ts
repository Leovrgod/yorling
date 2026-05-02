import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

test('defaults the app language to English for new installs', () => {
  const copySource = readFileSync('src/i18n/copy.ts', 'utf8');

  assert.ok(copySource.includes("export const DEFAULT_LANGUAGE: AppLanguageId = 'en';"));
});
