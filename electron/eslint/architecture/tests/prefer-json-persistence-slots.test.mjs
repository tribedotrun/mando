import { ruleTester } from '../../test-setup.mjs';
import rule from '../rules/prefer-json-persistence-slots.mjs';

ruleTester.run('architecture/prefer-json-persistence-slots', rule, {
  valid: [
    {
      code: `import { defineJsonKeyspace } from '#renderer/global/providers/persistence'; const store = defineJsonKeyspace('draft:', schema, 'owner');`,
      filename: 'src/renderer/domains/captain/runtime/useDraft.ts',
    },
    {
      code: `import { defineKeyspace } from '#renderer/global/providers/persistence'; const store = defineKeyspace('draft:', 'owner'); store.for('1').write('raw');`,
      filename: 'src/renderer/domains/captain/runtime/useDraft.ts',
    },
    {
      code: `JSON.parse(raw);`,
      filename: 'src/renderer/global/providers/http.ts',
    },
    {
      code: `JSON.parse(raw); JSON.stringify(value);`,
      filename: 'src/renderer/global/providers/persistence.ts',
    },
  ],
  invalid: [
    {
      code: `import { defineKeyspace } from '#renderer/global/providers/persistence'; const parsed = JSON.parse(raw);`,
      filename: 'src/renderer/domains/captain/runtime/useDraft.ts',
      errors: [{ messageId: 'useJsonSlots' }],
    },
    {
      code: `import { defineSlot } from '#renderer/global/providers/persistence'; const out = JSON.stringify(value);`,
      filename: 'src/renderer/global/runtime/foo.ts',
      errors: [{ messageId: 'useJsonSlots' }],
    },
  ],
});
