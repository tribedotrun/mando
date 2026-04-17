import { ruleTester } from '../../../test-setup.mjs';
import rule from '../rules/tsx-components-only.mjs';

const tsxFile = 'src/renderer/domains/captain/ui/Foo.tsx';
const tsFile = 'src/renderer/domains/captain/service/foo.ts';

ruleTester.run('arch/tsx-components-only', rule, {
  valid: [
    { code: `function Foo() { return <div />; }`, filename: tsxFile },
    { code: `const Foo = () => <div />;`, filename: tsxFile },
    { code: `const Foo = memo(() => <div />);`, filename: tsxFile },
    { code: `const Foo = forwardRef((props, ref) => <div />);`, filename: tsxFile },
    { code: `const Foo = React.memo(() => <div />);`, filename: tsxFile },
    { code: `export function Foo() { return <div />; }`, filename: tsxFile },
    { code: `export default function Foo() { return <div />; }`, filename: tsxFile },
    { code: `export const Foo = () => <div />;`, filename: tsxFile },
    { code: `const ZOOM_STEP = 0.3;`, filename: tsxFile },
    { code: `const CONFIG = { x: 1 };`, filename: tsxFile },
    { code: `type Props = { x: string };`, filename: tsxFile },
    { code: `interface Foo { x: string; }`, filename: tsxFile },
    { code: `const Icon = <svg />;`, filename: tsxFile },
    { code: `export * from './x';`, filename: tsxFile },
    { code: `export { X } from './x';`, filename: tsxFile },
    // .ts files are out of scope
    { code: `function parseBlocks() {}`, filename: tsFile },
    { code: `const useFoo = () => {};`, filename: tsFile },
  ],
  invalid: [
    {
      code: `function parseBlocks() {}`,
      filename: tsxFile,
      errors: [{ messageId: 'notComponent', data: { name: 'parseBlocks' } }],
    },
    {
      code: `const formatX = () => {};`,
      filename: tsxFile,
      errors: [{ messageId: 'notComponent', data: { name: 'formatX' } }],
    },
    {
      code: `const useFoo = () => {};`,
      filename: tsxFile,
      errors: [{ messageId: 'notComponent', data: { name: 'useFoo' } }],
    },
    {
      code: `function isValid() { return true; }`,
      filename: tsxFile,
      errors: [{ messageId: 'notComponent', data: { name: 'isValid' } }],
    },
    {
      code: `export function parseBlocks() {}`,
      filename: tsxFile,
      errors: [{ messageId: 'notComponent', data: { name: 'parseBlocks' } }],
    },
    {
      code: `function _helper() {}`,
      filename: tsxFile,
      errors: [{ messageId: 'notComponent', data: { name: '_helper' } }],
    },
  ],
});
