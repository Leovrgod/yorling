import { execFileSync } from 'node:child_process';
import { readdirSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { createRequire } from 'node:module';

const require = createRequire(import.meta.url);

function listCompiledTests(directory) {
  const entries = readdirSync(directory, { withFileTypes: true }).sort((left, right) =>
    left.name.localeCompare(right.name),
  );

  return entries.flatMap((entry) => {
    const entryPath = join(directory, entry.name);

    if (entry.isDirectory()) {
      return listCompiledTests(entryPath);
    }

    return entry.name.endsWith('.test.js') ? [entryPath] : [];
  });
}

rmSync('.tmp-tests', { recursive: true, force: true });

const tscEntrypoint = require.resolve('typescript/bin/tsc');

execFileSync(process.execPath, [tscEntrypoint, '-p', 'tsconfig.tests.json'], {
  stdio: 'inherit',
});

const compiledTests = listCompiledTests('.tmp-tests/tests');

if (compiledTests.length === 0) {
  throw new Error('No compiled tests were found under .tmp-tests/tests.');
}

execFileSync(process.execPath, ['--test', ...compiledTests], {
  stdio: 'inherit',
});
