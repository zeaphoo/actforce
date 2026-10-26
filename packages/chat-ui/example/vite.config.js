import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import { fileURLToPath } from 'node:url'
import { dirname, resolve } from 'node:path'

const __dirname = dirname(fileURLToPath(import.meta.url))

export default defineConfig({
  root: __dirname,
  plugins: [react()],
  resolve: {
    alias: {
      // Allow imports like `@actforce/chat-ui` to hit the local source directly.
      '@actforce/chat-ui': resolve(__dirname, '../src/index.js'),
    },
  },
  server: { port: 5180, open: false },
})
