import { ruleTester } from '../../test-setup.mjs';
import rule from '../rules/no-as-on-boundary.mjs';

ruleTester.run('mando/no-as-on-boundary', rule, {
  valid: [
    // as const is fine
    { code: `const x = { a: 1 } as const;` },
    // Cast on parsed Zod result is fine
    { code: `const x = schema.safeParse(raw) as Result;` },
    // Reason comment + double-cast (as unknown as T) escape hatch
    {
      code: `// reason: type lib bug, see #1234
        const x = JSON.parse(s) as unknown as Foo;`,
    },
    // Cast on a non-boundary value
    { code: `const x = 42 as number;` },
    // Cast inside .data of unrelated object
    { code: `const x = obj.data as Foo;` },
  ],
  invalid: [
    {
      code: `const x = JSON.parse(raw) as Foo;`,
      errors: [{ messageId: 'boundary' }],
    },
    {
      code: `const x = await res.json() as Foo;`,
      errors: [{ messageId: 'boundary' }],
    },
    {
      code: `const x = (await fetch(url)) as Foo;`,
      errors: [{ messageId: 'boundary' }],
    },
    {
      code: `const x = msg.data as Foo;`,
      errors: [{ messageId: 'boundary' }],
    },
    {
      code: `const x = ipcRenderer.invoke('ch') as Promise<Foo>;`,
      errors: [{ messageId: 'boundary' }],
    },
    {
      code: `const x = event.data.data as Foo;`,
      errors: [{ messageId: 'boundary' }],
    },
    // reason: comment alone does NOT unlock a plain single-cast on boundary data
    {
      code: `// reason: type lib bug
        const x = JSON.parse(s) as Foo;`,
      errors: [{ messageId: 'boundary' }],
    },
  ],
});
