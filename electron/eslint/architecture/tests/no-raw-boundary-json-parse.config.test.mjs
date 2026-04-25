import assert from 'node:assert/strict';
import path from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';
import { ESLint } from 'eslint';

const testsDir = path.dirname(fileURLToPath(import.meta.url));
const electronDir = path.resolve(testsDir, '../../..');

test('architecture/no-raw-boundary-json-parse is enabled across boundary processes', async () => {
  const eslint = new ESLint({
    cwd: electronDir,
    overrideConfigFile: path.join(electronDir, 'eslint.config.mjs'),
  });

  const cases = [
    'src/main/onboarding/runtime/setupValidation.ts',
    'src/preload/providers/ipc.ts',
    'src/renderer/global/providers/persistence.ts',
  ];

  for (const relativePath of cases) {
    const [result] = await eslint.lintText(`const raw = '{}'; JSON.parse(raw);`, {
      filePath: path.join(electronDir, relativePath),
    });
    assert(
      result.messages.some(
        (message) => message.ruleId === 'architecture/no-raw-boundary-json-parse',
      ),
      `${relativePath} should enable architecture/no-raw-boundary-json-parse`,
    );
  }
});
