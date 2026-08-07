import { defineConfig } from 'vitest/config'

/**
 * Kept separate from `vite.config.ts` so `tsc -b` typechecks the app config
 * against Vite's own `UserConfig` (which has no `test` key) without needing a
 * triple-slash reference in a file the build also consumes.
 */
export default defineConfig({
  test: {
    environment: 'jsdom',
    include: ['src/**/*.test.ts', 'src/**/*.test.tsx'],
  },
})
