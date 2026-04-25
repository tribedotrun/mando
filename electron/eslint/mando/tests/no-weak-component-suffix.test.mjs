import { ruleTester } from '../../test-setup.mjs';
import rule from '../rules/no-weak-component-suffix.mjs';

const base = '/abs/src/renderer/domains/captain/ui/';

ruleTester.run('mando/no-weak-component-suffix', rule, {
  valid: [
    // Canonical Frame passes.
    { code: `export function TaskFrame() { return <div />; }`, filename: `${base}TaskFrame.tsx` },
    // Canonical Parts passes.
    { code: `export function TaskParts() { return <div />; }`, filename: `${base}TaskParts.tsx` },
    // Helpers as .ts (no JSX) passes even with JSX-banned suffix.
    { code: `export function foo() { return 1; }`, filename: `${base}TaskHelpers.ts` },
    // Primitives directory is exempt.
    { code: `export function Button() { return <button />; }`, filename: `/abs/src/renderer/global/ui/primitives/button-shell.tsx` },
    // Non-renderer file is ignored.
    { code: `export function Foo() { return <div />; }`, filename: `/abs/src/main/FooSurface.tsx` },
    // Short suffix-only stem (e.g. exactly "Shell") is not banned since the
    // suffix check requires the stem to be longer than the suffix. This
    // prevents accidental flags on domain words like "Host" used as a name.
    { code: `export function Shell() { return <div />; }`, filename: `${base}Shell.tsx` },
    // Helpers file with no JSX is valid.
    { code: `export function taskHelpers() { return 1; }`, filename: `${base}TaskHelpers.tsx` },
  ],
  invalid: [
    {
      code: `export function QAChatSurface() { return <div />; }`,
      filename: `${base}QAChatSurface.tsx`,
      errors: [{ messageId: 'bannedSuffix' }],
    },
    {
      code: `export function CardShell() { return <div />; }`,
      filename: `${base}CardShell.tsx`,
      errors: [{ messageId: 'bannedSuffix' }],
    },
    {
      code: `export function Host() { return <div />; } export function FooHost() { return <div />; }`,
      filename: `${base}FooHost.tsx`,
      errors: [{ messageId: 'bannedSuffix' }],
    },
    {
      code: `export function ImageLightboxChrome() { return <div />; }`,
      filename: `${base}ImageLightboxChrome.tsx`,
      errors: [{ messageId: 'bannedSuffix' }],
    },
    // Helpers file that renders JSX is banned.
    {
      code: `export function SessionsEmptyState() { return <div />; }`,
      filename: `${base}SessionsHelpers.tsx`,
      errors: [{ messageId: 'jsxInHelpersOrData' }],
    },
    // Data file that contains JSX is banned.
    {
      code: `export function CommandRow() { return <div />; } export const X = 1;`,
      filename: `${base}CommandPaletteData.tsx`,
      errors: [{ messageId: 'jsxInHelpersOrData' }],
    },
  ],
});
