// Tests legitimately need hardcoded values, fetch mocks, cross-domain imports,
// and direct DOM assertions. Disable architecture and purity rules under tests/.

export default [
  {
    files: ['tests/**/*.ts', 'tests/**/*.tsx'],
    rules: {
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
    },
  },
];
