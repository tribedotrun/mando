// Tests legitimately need fetch mocks, direct DOM assertions, and broad test fixtures.

const TEST_RULE_OVERRIDES = {
  'no-restricted-imports': 'off',
  '@typescript-eslint/no-explicit-any': 'off',
};

export default [
  {
    files: ['tests/**/*.ts', 'tests/**/*.tsx'],
    rules: TEST_RULE_OVERRIDES,
  },
  // Unit tests under src/**/__tests__/ (node:test runner). Same overrides plus floating-promises
  // since node:test's describe/it return promises that the runner consumes.
  {
    files: ['src/**/__tests__/**/*.ts', 'src/**/*.test.ts'],
    rules: {
      ...TEST_RULE_OVERRIDES,
      '@typescript-eslint/no-floating-promises': 'off',
      '@typescript-eslint/no-misused-promises': 'off',
    },
  },
];
