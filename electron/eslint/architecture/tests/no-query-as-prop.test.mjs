import { ruleTester } from '../../test-setup.mjs';
import rule from '../rules/no-query-as-prop.mjs';

ruleTester.run('architecture/no-query-as-prop', rule, {
  valid: [
    {
      code: `import { useScoutItem } from '#renderer/domains/scout/runtime/hooks'; function View(){ const { data: item } = useScoutItem(1); return <Child itemId={item?.id} />; }`,
      filename: 'src/renderer/domains/scout/ui/View.tsx',
    },
    {
      code: `function View({ itemId }){ return <Child itemId={itemId} />; }`,
      filename: 'src/renderer/domains/scout/ui/View.tsx',
    },
    {
      code: `import { useScoutItem } from '#renderer/domains/scout/runtime/hooks'; function One(){ const { data } = useScoutItem(1); return <Child itemId={data?.id} />; } function Two(){ const data = localValue; return <Child item={data} />; }`,
      filename: 'src/renderer/domains/scout/ui/View.tsx',
    },
    {
      code: `import { useScoutItem } from '#renderer/domains/scout/runtime/hooks'; function View(){ const { data } = useScoutItem(1); const render = (data) => <Child item={data} />; return <>{render(null)}</>; }`,
      filename: 'src/renderer/domains/scout/ui/View.tsx',
    },
    {
      code: `import { useScoutItem } from '#renderer/domains/scout/runtime/hooks'; function View(){ const { data } = useScoutItem(1); const renderItem = (data) => <Child item={data} />; return <List renderItem={renderItem} />; }`,
      filename: 'src/renderer/domains/scout/ui/View.tsx',
    },
  ],
  invalid: [
    {
      code: `import { useScoutItem } from '#renderer/domains/scout/runtime/hooks'; function View(){ const { data: item } = useScoutItem(1); return <Child item={item} />; }`,
      filename: 'src/renderer/domains/scout/ui/View.tsx',
      errors: [{ messageId: 'queryProp' }],
    },
    {
      code: `import { useCredentialsList } from '#renderer/domains/settings/runtime/hooks'; function View(){ const { data } = useCredentialsList(); const credentials = data?.credentials ?? []; return <Rows credentials={credentials} />; }`,
      filename: 'src/renderer/domains/settings/ui/View.tsx',
      errors: [{ messageId: 'queryProp' }],
    },
    {
      code: `import { useScoutItem } from '#renderer/domains/scout/runtime/hooks'; function View(){ const { data } = useScoutItem(1); const item = data?.item; return <Child item={item} />; }`,
      filename: 'src/renderer/domains/scout/ui/View.tsx',
      errors: [{ messageId: 'queryProp' }],
    },
    {
      code: `import { useScoutItem } from '#renderer/domains/scout/runtime/hooks'; function View(){ const { data } = useScoutItem(1); const item = useMemo(() => data?.item, [data]); return <Child item={item} />; }`,
      filename: 'src/renderer/domains/scout/ui/View.tsx',
      errors: [{ messageId: 'queryProp' }],
    },
  ],
});
