import js from '@eslint/js'
import reactHooks from 'eslint-plugin-react-hooks'
import globals from 'globals'
import tseslint from 'typescript-eslint'

import noJsxLiteral from './eslint-rules/no-jsx-literal.js'

export default tseslint.config(
  { ignores: ['dist', 'src-tauri', 'target', 'coverage', '.claude'] },
  {
    files: ['src/**/*.{ts,tsx}'],
    extends: [js.configs.recommended, ...tseslint.configs.recommended],
    languageOptions: { ecmaVersion: 2023, globals: globals.browser },
    plugins: {
      'react-hooks': reactHooks,
      local: { rules: { 'no-jsx-literal': noJsxLiteral } },
    },
    rules: {
      ...reactHooks.configs.recommended.rules,
      // All user-visible text must come from locale files via t('key').
      'local/no-jsx-literal': 'error',
    },
  },
  {
    files: ['src/**/*.test.{ts,tsx}', 'src/test/**'],
    rules: { 'local/no-jsx-literal': 'off' },
  },
)
