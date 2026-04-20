import { ruleTester } from '../../test-setup.mjs';
import rule from '../rules/no-raw-boundary-json-parse.mjs';

ruleTester.run('architecture/no-raw-boundary-json-parse', rule, {
  valid: [
    {
      code: `const parsed = parseJsonText(raw, 'file:test');`,
      filename: 'src/main/updater/service/channelConfig.ts',
    },
    {
      code: `const parsed = parseJsonTextWith(raw, schema, 'file:test');`,
      filename: 'src/renderer/global/repo/queries.ts',
    },
    {
      code: `const parsed = parseSseMessage(message.data, schema);`,
      filename: 'src/renderer/domains/captain/repo/terminal-api.ts',
    },
    {
      code: `const parsed = parseJsonTextWith(raw, schema, 'persistence:test');`,
      filename: 'src/renderer/global/providers/persistence.ts',
    },
    {
      code: `JSON.parse(raw);`,
      filename: 'src/renderer/domains/sessions/ui/TranscriptBlocks.tsx',
    },
    { code: `JSON.parse(raw);`, filename: 'src/shared/result/helpers.ts' },
  ],
  invalid: [
    {
      code: `const parsed = JSON.parse(raw);`,
      filename: 'src/main/updater/service/channelConfig.ts',
      errors: [{ messageId: 'useBoundaryHelper' }],
    },
    {
      code: `const parsed = JSON.parse(raw);`,
      filename: 'src/renderer/global/repo/queries.ts',
      errors: [{ messageId: 'useBoundaryHelper' }],
    },
    {
      code: `const parsed = JSON.parse(raw);`,
      filename: 'src/shared/ipc-contract/schemas.ts',
      errors: [{ messageId: 'useBoundaryHelper' }],
    },
    {
      code: `const parsed = JSON.parse(raw);`,
      filename: 'src/renderer/global/providers/persistence.ts',
      errors: [{ messageId: 'useBoundaryHelper' }],
    },
  ],
});
