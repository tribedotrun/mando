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
    {
      code: `const parsed = JSON.parse(raw);`,
      filename: 'src/preload/providers/ipc.ts',
      errors: [{ messageId: 'useBoundaryHelper' }],
    },
    {
      code: `const parsed = JSON.parse(raw);`,
      filename: 'src/renderer/domains/sessions/ui/TranscriptBlocks.tsx',
      errors: [{ messageId: 'useBoundaryHelper' }],
    },
    {
      code: `const parsed = globalThis.JSON.parse(raw);`,
      filename: 'src/renderer/global/repo/queries.ts',
      errors: [{ messageId: 'useBoundaryHelper' }],
    },
    {
      code: `const parsed = JSON['parse'](raw);`,
      filename: 'src/main/updater/service/channelConfig.ts',
      errors: [{ messageId: 'useBoundaryHelper' }],
    },
    {
      code: `const parse = JSON.parse; const parsed = parse(raw);`,
      filename: 'src/renderer/global/repo/queries.ts',
      errors: [{ messageId: 'useBoundaryHelper' }],
    },
    {
      code: `const { parse } = JSON; const parsed = parse(raw);`,
      filename: 'src/renderer/domains/sessions/ui/TranscriptBlocks.tsx',
      errors: [{ messageId: 'useBoundaryHelper' }],
    },
    {
      code: `const parsed = (0, JSON.parse)(raw);`,
      filename: 'src/renderer/global/repo/queries.ts',
      errors: [{ messageId: 'useBoundaryHelper' }],
    },
    {
      code: `const parsed = JSON.parse.call(null, raw);`,
      filename: 'src/renderer/global/repo/queries.ts',
      errors: [{ messageId: 'useBoundaryHelper' }],
    },
    {
      code: `const parsed = Reflect.apply(JSON.parse, null, [raw]);`,
      filename: 'src/renderer/global/repo/queries.ts',
      errors: [{ messageId: 'useBoundaryHelper' }],
    },
    {
      code: `const parse = JSON.parse.bind(JSON); const parsed = parse(raw);`,
      filename: 'src/preload/providers/ipc.ts',
      errors: [{ messageId: 'useBoundaryHelper' }],
    },
    {
      code: `const parsed = JSON.parse.bind(globalThis.JSON)(raw);`,
      filename: 'src/main/onboarding/runtime/setupValidation.ts',
      errors: [{ messageId: 'useBoundaryHelper' }],
    },
    {
      code: `const raw = await response.json();`,
      filename: 'src/renderer/global/providers/http.ts',
      errors: [{ messageId: 'useBoundaryHelper' }],
    },
    {
      code: `const raw = await response.json();`,
      filename: 'src/main/global/runtime/lifecycle.ts',
      errors: [{ messageId: 'useBoundaryHelper' }],
    },
  ],
});
