const tsParser = require('@typescript-eslint/parser');
const tsPlugin = require('@typescript-eslint/eslint-plugin');

module.exports = [
  {
    files: ['js/**/*.ts'],
    languageOptions: {
      parser: tsParser,
      parserOptions: {
        ecmaVersion: 2022,
        sourceType: 'module',
      },
    },
    plugins: {
      '@typescript-eslint': tsPlugin,
    },
    rules: {
      ...tsPlugin.configs.recommended.rules,
      '@typescript-eslint/no-unused-vars': ['warn', { argsIgnorePattern: '^_' }],
      '@typescript-eslint/no-explicit-any': 'warn',
      '@typescript-eslint/no-non-null-assertion': 'off',
      '@typescript-eslint/no-require-imports': 'off', // napi-rs binding loader uses require()
      'no-console': 'off',
    },
  },
  {
    files: ['tests/**/*.ts'],
    languageOptions: {
      parser: tsParser,
      parserOptions: {
        ecmaVersion: 2022,
        sourceType: 'module',
      },
    },
    plugins: {
      '@typescript-eslint': tsPlugin,
    },
    rules: {
      ...tsPlugin.configs.recommended.rules,
      '@typescript-eslint/no-unused-vars': ['warn', { argsIgnorePattern: '^_' }],
      '@typescript-eslint/no-explicit-any': 'off', // test fixtures intentionally use any
      '@typescript-eslint/no-non-null-assertion': 'off',
      '@typescript-eslint/no-require-imports': 'off',
      'no-console': 'off',
    },
  },
  {
    ignores: ['dist/', 'node_modules/', 'target/', 'qualitas_napi.js', 'qualitas_napi.d.ts'],
  },
];
