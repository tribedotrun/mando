// Tests legitimately need hardcoded values, fetch mocks, cross-domain imports,
// and direct DOM assertions. Disable architecture and purity rules under tests/.

const TEST_RULE_OVERRIDES = {
  'no-restricted-imports': 'off',
  'no-restricted-syntax': 'off',
  'design-system/no-hardcoded-colors': 'off',
  'design-system/no-off-scale-font-size': 'off',
  'design-system/no-off-grid-radius': 'off',
  'arch/no-api-in-components': 'off',
  'arch/no-business-logic-in-ui': 'off',
  'arch/no-network-in-ui': 'off',
  'mando/no-direct-dom-mutation': 'off',
  'mando/no-magic-timeouts': 'off',
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
