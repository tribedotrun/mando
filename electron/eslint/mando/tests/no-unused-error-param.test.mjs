import { ruleTester } from '../../test-setup.mjs';
import rule from '../rules/no-unused-error-param.mjs';

ruleTester.run('mando/no-unused-error-param', rule, {
  valid: [
    // try/catch: error referenced in body
    {
      code: `try { doThing(); } catch (err) { console.log(err); }`,
    },
    // try/catch: logger call satisfies the rule even if err itself is unused
    {
      code: `try { doThing(); } catch (_err) { log.error('failed'); }`,
    },
    // Promise.catch: error referenced
    {
      code: `promise().catch((err) => { handle(err); });`,
    },
    // Promise.catch: logger call
    {
      code: `promise().catch((_err) => { logger.warn('failed'); });`,
    },
    // useMutation onError: error referenced
    {
      code: `useMutation({ onError: (err) => { report(err); } });`,
    },
    // useMutation onError: logger call
    {
      code: `useMutation({ onError: (_err) => { log.error('op failed'); } });`,
    },
    // useMutation onSettled: error referenced
    {
      code: `useMutation({ onSettled: (data, err, vars) => { if (err) handle(err); } });`,
    },
    // useMutation onSuccess: positional args beyond first are not constrained
    {
      code: `useMutation({ onSuccess: (data, variables, context) => { use(data); } });`,
    },
    // Outside the scoped shapes — underscore prefix still works
    {
      code: `function handler(_err) { doSomething(); }`,
    },
    // Destructured catch parameter with at least one field referenced
    {
      code: `try { doThing(); } catch ({ message }) { toast(message); }`,
    },
    // Destructured onError with logger call
    {
      code: `useMutation({ onError: ({ message }) => { log.error('op', message); } });`,
    },
  ],
  invalid: [
    // try/catch: unreferenced err, no logger call
    {
      code: `try { doThing(); } catch (err) { toast('boom'); }`,
      errors: [{ messageId: 'unused' }],
    },
    // Promise.catch: unreferenced err, no logger
    {
      code: `promise().catch((err) => { toast('boom'); });`,
      errors: [{ messageId: 'unused' }],
    },
    // useMutation onError: unreferenced _err, no logger
    {
      code: `useMutation({ onError: (_err) => { toast('failed'); } });`,
      errors: [{ messageId: 'unused' }],
    },
    // useMutation onSettled: unreferenced err (index 1), no logger
    {
      code: `useMutation({ onSettled: (data, err, vars) => { invalidate(vars); } });`,
      errors: [{ messageId: 'unused' }],
    },
    // Destructured catch with no field referenced and no logger call
    {
      code: `try { doThing(); } catch ({ message }) { toast('fail'); }`,
      errors: [{ messageId: 'unusedDestructured' }],
    },
    // Destructured onError where no destructured field is referenced
    {
      code: `useMutation({ onError: ({ message, code }) => { toast('fail'); } });`,
      errors: [{ messageId: 'unusedDestructured' }],
    },
  ],
});
