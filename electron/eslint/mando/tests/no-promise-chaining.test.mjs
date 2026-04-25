import { ruleTester } from '../../test-setup.mjs';
import rule from '../rules/no-promise-chaining.mjs';

ruleTester.run('mando/no-promise-chaining', rule, {
  valid: [
    {
      code: `async function save() { try { await write(); } catch (err) { log.warn('failed', err); } }`,
    },
    {
      code: `import { z } from 'zod'; const schema = z.string().optional().catch(undefined);`,
    },
    {
      code: `import { z as schema } from 'zod'; const value = schema.coerce.number().int().catch(0);`,
    },
    {
      code: `import { z } from 'zod'; const schema = z.string().optional(); const value = schema.catch(undefined);`,
    },
    {
      code: `import * as z from 'zod'; const schema = z.string().optional(); const value = schema.catch(undefined);`,
    },
    {
      filename: '/repo/electron/src/shared/result/async-result.ts',
      code: `export function wrap(promise) { return promise.then((value) => value); }`,
    },
  ],
  invalid: [
    {
      code: `function save() { return write().then(() => done()); }`,
      errors: [{ messageId: 'promiseChain' }],
    },
    {
      code: `function save() { void write().catch((err) => log.warn(err)); }`,
      errors: [{ messageId: 'promiseChain' }],
    },
    {
      code: `function save() { return write().then(done).catch(failed); }`,
      errors: [{ messageId: 'promiseChain' }, { messageId: 'promiseChain' }],
    },
    {
      code: `import { z } from 'zod'; const parsed = z.string().parseAsync(raw); parsed.catch(handle);`,
      errors: [{ messageId: 'promiseChain' }],
    },
  ],
});
