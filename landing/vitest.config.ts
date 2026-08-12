import { defineConfig } from 'vitest/config'
import { fileURLToPath } from 'node:url'

export default defineConfig({
  resolve: {
    alias: {
      '@': fileURLToPath(new URL('./src', import.meta.url)),
    },
  },
  test: {
    /* Node, not jsdom: everything under test here is arithmetic. The scene
       modules are deliberately free of three, React and the DOM so this stays
       true — the moment a test needs a browser environment, something that
       should have been pure has stopped being pure. */
    environment: 'node',
    include: ['src/tests/**/*.test.ts'],
  },
})
