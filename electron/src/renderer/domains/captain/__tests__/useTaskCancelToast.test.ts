// Regression test for #111: clicking "Cancel task" in the workbench header
// overflow menu produced no observable user-visible feedback. The mutation
// fires correctly and the task transitions to `canceled`, but the only
// signal was a small status-badge change in the workbench header, easy
// to miss and report as "the button does nothing."
//
// Fix: `useTaskCancel` declares an `onSuccess` toast (`Task canceled`)
// to mirror the success-toast pattern in sibling lifecycle mutations
// (`useTaskReopen`, `useTaskRework`, `useTaskMerge`, `useTaskCreate`).
//
// This file fences the mutation-feedback configuration at source level,
// because the renderer has no React-render harness (see FeedClarifierCard
// test). The pre-fix declaration had only `onError`; the post-fix
// declaration has both `onSuccess` and `onError`.

import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const __dirname = dirname(fileURLToPath(import.meta.url));
const mutationsPath = join(__dirname, '..', 'runtime', 'useFeedbackTaskMutations.ts');
const source = readFileSync(mutationsPath, 'utf8');

function functionBody(name: string): string {
  const decl = `export function ${name}(`;
  const start = source.indexOf(decl);
  assert.notEqual(start, -1, `expected to find ${decl} in ${mutationsPath}`);
  const open = source.indexOf('{', start);
  let depth = 0;
  for (let i = open; i < source.length; i++) {
    const ch = source[i];
    if (ch === '{') depth++;
    else if (ch === '}') {
      depth--;
      if (depth === 0) return source.slice(open, i + 1);
    }
  }
  // invariant: brace scan stops only when depth returns to 0; reaching here means the source is malformed and the test must fail
  assert.fail(`unterminated function body for ${name}`);
}

describe('useTaskCancel mutation feedback (regression #111)', () => {
  it('declares an onSuccess handler that toasts user-visible confirmation', () => {
    const body = functionBody('useTaskCancel');
    assert.match(
      body,
      /onSuccess:\s*\(\s*\)\s*=>\s*\{\s*toast\.success\(/,
      'useTaskCancel must wire an onSuccess toast: the workbench header has no other ' +
        'click-time feedback when a task transitions to canceled, so a missing onSuccess ' +
        'leaves the user staring at an unchanged screen (regression that started bug #111).',
    );
  });

  it('still declares an onError handler for failed cancels', () => {
    const body = functionBody('useTaskCancel');
    assert.match(body, /onError:\s*\(\s*\)\s*=>\s*\{\s*toast\.error\(/);
  });
});
