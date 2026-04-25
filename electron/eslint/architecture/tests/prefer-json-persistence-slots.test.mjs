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
      code: `import { defineJsonSlot } from '#renderer/global/providers/persistence'; const slot = defineJsonSlot('flag', schema, 'owner'); slot.read() ?? true;`,
      filename: 'src/renderer/global/runtime/useFlags.ts',
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
    {
      code: `import { defineSlot } from '#renderer/global/providers/persistence'; const enabled = slot.read() !== 'false';`,
      filename: 'src/renderer/global/runtime/useFlags.ts',
      errors: [{ messageId: 'useTypedSlots' }],
    },
    {
      code: `import { defineKeyspace } from '#renderer/global/providers/persistence'; store.for('bulk').write('1');`,
      filename: 'src/renderer/domains/captain/runtime/useDraft.ts',
      errors: [{ messageId: 'useTypedSlots' }],
    },
  ],
});
