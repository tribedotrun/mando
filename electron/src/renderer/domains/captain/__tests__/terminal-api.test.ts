import assert from 'node:assert/strict';
import { describe, it } from 'node:test';
import { decodeTerminalOutputPayload } from '../repo/terminalOutput.ts';

describe('decodeTerminalOutputPayload', () => {
  it('rejects malformed terminal base64 payload before decode', () => {
    const result = decodeTerminalOutputPayload({ dataB64: '@@@' }, 'test:terminal-output');

    result.match(
      () => assert.fail('expected parse error on malformed base64'),
      (error) => assert.equal(error.code, 'parse'),
    );
  });

  it('decodes valid base64 payload into bytes', () => {
    const result = decodeTerminalOutputPayload({ dataB64: 'aGk=' }, 'test:terminal-output');

    result.match(
      (bytes) => assert.deepEqual(Array.from(bytes), [104, 105]),
      () => assert.fail('expected valid terminal payload'),
    );
  });
});
