import assert from 'node:assert/strict';
import { describe, it } from 'node:test';
import { wrapAsciiArt } from '../service/prHelpers.ts';

const WORK_SUMMARY_DIAGRAM = [
  'User opens composer  ─→  useTextImageDraft(key, {initialText, legacyTextSuffix})',
  '(10 surfaces)              │',
  '                           ▼',
  '┌──────────────────────────────────────────────────────────────────┐',
  '│ useTextImageDraft (renderer/global/runtime)                      │',
  '│                                                                  │',
  '└──────────────────────────────────────────────────────────────────┘',
].join('\n');

describe('wrapAsciiArt', () => {
  it('preserves already fenced work summary diagrams', () => {
    const markdown = ['```', WORK_SUMMARY_DIAGRAM, '```', '', '**What changed**: fixed it.'].join(
      '\n',
    );

    assert.equal(wrapAsciiArt(markdown), markdown);
  });

  it('keeps fenced work summary diagrams as one populated code block', () => {
    const markdown = ['```', WORK_SUMMARY_DIAGRAM, '```', '', '**What changed**: fixed it.'].join(
      '\n',
    );

    const wrapped = wrapAsciiArt(markdown);
    const lines = wrapped.split('\n');

    assert.equal(wrapped.split('```').length - 1, 2);
    assert.equal(lines[0], '```');
    assert.match(lines[1], /^User opens composer/);
    assert.equal(
      lines[lines.indexOf('```', 1) - 1],
      '└──────────────────────────────────────────────────────────────────┘',
    );
  });

  it('wraps unfenced box-drawing diagrams once', () => {
    const wrapped = wrapAsciiArt(`${WORK_SUMMARY_DIAGRAM}\n\n**What changed**: fixed it.`);

    assert.equal(wrapped.split('```').length - 1, 2);
    assert.match(wrapped, /^```\nUser opens composer/);
    assert.match(
      wrapped,
      /└──────────────────────────────────────────────────────────────────┘\n```/,
    );
  });

  it('requires markdown fences to start within the first three spaces', () => {
    const markdown = ['    ```', WORK_SUMMARY_DIAGRAM, '    ```'].join('\n');
    const wrapped = wrapAsciiArt(markdown);

    assert.match(wrapped, /^[ ]{4}```\n```\nUser opens composer/);
    assert.match(
      wrapped,
      /└──────────────────────────────────────────────────────────────────┘\n```\n[ ]{4}```$/,
    );
  });
});
