import js from '@eslint/js'
import globals from 'globals'
import reactHooks from 'eslint-plugin-react-hooks'
import reactRefresh from 'eslint-plugin-react-refresh'
import tseslint from 'typescript-eslint'
import { defineConfig, globalIgnores } from 'eslint/config'

export default defineConfig([
  globalIgnores([
    'dist',
    // Rust build output: `tauri-codegen-assets` contains minified/binary `.js`
    // blobs that are not parseable source and are not ours to lint.
    'src-tauri/target',
    // Fixture inputs are deliberately malformed — `broken.ts` exists to prove
    // the parser degrades gracefully.
    'src-tauri/tests/fixtures',
  ]),
  {
    files: ['**/*.{ts,tsx}'],
    extends: [
      js.configs.recommended,
      tseslint.configs.recommended,
      reactHooks.configs.flat.recommended,
      reactRefresh.configs.vite,
    ],
    languageOptions: {
      globals: globals.browser,
    },
  },
])
