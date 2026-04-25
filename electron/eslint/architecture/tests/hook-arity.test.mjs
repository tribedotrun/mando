import { ruleTester } from '../../test-setup.mjs';
import rule from '../rules/hook-arity.mjs';

ruleTester.run('architecture/hook-arity', rule, {
  valid: [
    {
      code: `export function useThing(){ return { a:1,b:2,c:3,d:4,e:5,f:6,g:7,h:8 }; }`,
      filename: 'src/renderer/global/runtime/useThing.ts',
    },
    {
      code: `export function useThing(){ return useQuery({ queryKey: ['x'], queryFn }); }`,
      filename: 'src/renderer/global/repo/queries.ts',
    },
    {
      code: `export function useThing(){ const mutation = useMutation({ mutationFn }); return { ...mutation, label: 'x' }; }`,
      filename: 'src/renderer/global/repo/mutations.ts',
    },
    {
      code: `export function useThing(){ const process = useCallback(() => { return { a:1,b:2,c:3,d:4,e:5,f:6,g:7,h:8,i:9 }; }, []); return { process }; }`,
      filename: 'src/renderer/global/runtime/useThing.ts',
    },
  ],
  invalid: [
    {
      code: `export function useThing(){ return { a:1,b:2,c:3,d:4,e:5,f:6,g:7,h:8,i:9 }; }`,
      filename: 'src/renderer/global/runtime/useThing.ts',
      errors: [{ messageId: 'tooMany' }],
    },
    {
      code: `export function useThing(){ const extra = {}; return { a:1, ...extra }; }`,
      filename: 'src/renderer/global/runtime/useThing.ts',
      errors: [{ messageId: 'opaqueSpread' }],
    },
    {
      code: `export const useThing = () => ({ a:1,b:2,c:3,d:4,e:5,f:6,g:7,h:8,i:9 });`,
      filename: 'src/renderer/global/runtime/useThing.ts',
      errors: [{ messageId: 'tooMany' }],
    },
  ],
});
