import { ruleTester } from '../../test-setup.mjs';
import rule from '../rules/require-schema-on-funnel.mjs';

ruleTester.run('mando/require-schema-on-funnel', rule, {
  valid: [
    { code: `parseWith(taskItemSchema, raw, 'where');` },
    { code: `fromResponse(p, askResponseSchema, 'route');` },
    { code: `fromIpc('ch', p, pendingUpdateInfoSchema);` },
    { code: `fromSseMessage(raw, sseEnvelopeSchema, 'sse');` },
    { code: `fromFile(read, '/p', mandoConfigSchema);` },
    { code: `someOtherCall(arg);` },
  ],
  invalid: [
    {
      code: `parseWith(raw, 'where');`,
      errors: [{ messageId: 'missing' }],
    },
    {
      code: `fromResponse(p, 'route');`,
      errors: [{ messageId: 'missing' }],
    },
    {
      code: `fromIpc('ch', p);`,
      errors: [{ messageId: 'missing' }],
    },
  ],
});
