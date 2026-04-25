import { ruleTester } from '../../test-setup.mjs';
import rule from '../rules/filename-matches-export.mjs';

const base = '/abs/src/renderer/domains/captain/ui/';

ruleTester.run('mando/filename-matches-export', rule, {
  valid: [
    // Filename stem matches a named export.
    {
      code: `export function TaskRow() { return null; }`,
      filename: `${base}TaskRow.tsx`,
    },
    // Filename stem matches one of several exports.
    {
      code: `export function TaskRow() {} export function Helper() {}`,
      filename: `${base}TaskRow.tsx`,
    },
    // Default export identifier matches stem.
    {
      code: `export default function TaskRow() { return null; }`,
      filename: `${base}TaskRow.tsx`,
    },
    // index.tsx is exempt.
    {
      code: `export { Foo } from './Foo';`,
      filename: `${base}index.tsx`,
    },
    // index.ts is exempt.
    {
      code: `export { Foo } from './Foo';`,
      filename: `${base}index.ts`,
    },
    // Primitives directory is exempt.
    {
      code: `export function Button() {}`,
      filename: `/abs/src/renderer/global/ui/primitives/button.tsx`,
    },
    // Outside renderer is ignored.
    {
      code: `export function Bar() {}`,
      filename: `/abs/src/main/foo.ts`,
    },
    // Re-export using the stem.
    {
      code: `export { TaskRow } from './elsewhere';`,
      filename: `${base}TaskRow.tsx`,
    },
    // .ts files are out of scope (utilities, services — grab-bag allowed).
    {
      code: `export function a() {} export function b() {}`,
      filename: `/abs/src/renderer/global/service/utils.ts`,
    },
    // Tests directory is exempt.
    {
      code: `export function helper() {}`,
      filename: `${base}__tests__/helper.test.tsx`,
    },
  ],
  invalid: [
    // Filename says TaskFormControls but only exports TaskFormFooter.
    {
      code: `export function TaskFormFooter() { return null; } export function BulkTaskFormFooter() { return null; }`,
      filename: `${base}TaskFormControls.tsx`,
      errors: [{ messageId: 'stemMismatch' }],
    },
    // Grab bag: stem is not among exports.
    {
      code: `export function MoreIcon() {} export function ArchiveBtn() {} export function MergeBtn() {}`,
      filename: `${base}TaskIcons.tsx`,
      errors: [{ messageId: 'stemMismatch' }],
    },
    // Anonymous arrow default export — file does have an export, but no identifier.
    {
      code: `export default () => null;`,
      filename: `${base}TaskRow.tsx`,
      errors: [{ messageId: 'anonDefault' }],
    },
    // Truly empty file with no exports.
    {
      code: `const x = 1;`,
      filename: `${base}TaskRow.tsx`,
      errors: [{ messageId: 'noExports' }],
    },
    // Named re-export under a different name.
    {
      code: `import { Foo } from './Foo'; export { Foo as Bar };`,
      filename: `${base}OtherName.tsx`,
      errors: [{ messageId: 'stemMismatch' }],
    },
  ],
});
