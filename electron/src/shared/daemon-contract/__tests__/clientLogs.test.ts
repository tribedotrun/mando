import assert from 'node:assert/strict';
import { describe, it } from 'node:test';

import { normalizeClientLogContext } from '../clientLogs.ts';
import { assertRouteBody } from '../runtime.ts';

describe('client-log normalization', () => {
  it('maps arbitrary context into the shared client-log wire schema', () => {
    const context = normalizeClientLogContext({
      error: 'boom',
      issues: [{ code: 'custom', message: 'bad payload', path: ['entries', 0] }],
      route: 'getSessions',
    });

    assert.equal(context?.route, 'getSessions');
    assert.match(context?.extra ?? '', /boom/);
    assert.doesNotThrow(() =>
      assertRouteBody('postClientlogs', {
        entries: [
          {
            level: 'error',
            message: 'request failed',
            context,
            timestamp: new Date().toISOString(),
          },
        ],
      }),
    );
  });
});
