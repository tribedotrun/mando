// Wires ESLint's RuleTester to node:test so each rule's invalid/valid cases
// surface as discrete test results.
import { RuleTester } from 'eslint';
import { describe, it } from 'node:test';
import tsParser from '@typescript-eslint/parser';

RuleTester.describe = describe;
RuleTester.it = it;
RuleTester.itOnly = it.only;

export const ruleTester = new RuleTester({
  languageOptions: {
    parser: tsParser,
    ecmaVersion: 2024,
    sourceType: 'module',
    parserOptions: { ecmaFeatures: { jsx: true } },
  },
});
