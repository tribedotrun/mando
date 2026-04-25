import { ruleTester } from '../../test-setup.mjs';
import rule from '../rules/no-mailbox-prop.mjs';

ruleTester.run('architecture/no-mailbox-prop', rule, {
  valid: [
    {
      code: `function Parent(){ const { item } = useThing(); return <Child itemId={item.id} />; }`,
      filename: 'src/renderer/domains/scout/ui/Parent.tsx',
    },
    {
      code: `function Parent({ onClose }){ return <Child onClose={onClose} />; }`,
      filename: 'src/renderer/domains/scout/ui/Parent.tsx',
    },
    {
      code: `function Parent(){ const { item } = useThing(); return item ? <Child item={item} /> : null; }`,
      filename: 'src/renderer/domains/scout/ui/Parent.tsx',
    },
  ],
  invalid: [
    {
      code: `function Parent(){ const { item } = useThing(); return <Child item={item} />; }`,
      filename: 'src/renderer/domains/scout/ui/Parent.tsx',
      errors: [{ messageId: 'mailbox' }],
    },
    {
      code: `function Parent(){ const [open, setOpen] = useState(false); return <Child open={open} />; }`,
      filename: 'src/renderer/domains/scout/ui/Parent.tsx',
      errors: [{ messageId: 'mailbox' }],
    },
    {
      code: `function Parent(){ const { item } = useThing(); const onKey = (e) => { if (e.key !== 'Escape') return; }; return <Child item={item} />; }`,
      filename: 'src/renderer/domains/scout/ui/Parent.tsx',
      errors: [{ messageId: 'mailbox' }],
    },
  ],
});
