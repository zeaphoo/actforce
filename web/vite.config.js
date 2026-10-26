import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import legacy from '@vitejs/plugin-legacy'
import path from 'path'

export default defineConfig({
  plugins: [
    react(),
    // Old-browser compatibility (same as agentlite web: Chrome 68+)
    legacy({
      targets: ['chrome >= 68'],
      modernTargets: ['chrome >= 68'],
      additionalLegacyPolyfills: ['regenerator-runtime/runtime'],
      modernPolyfills: true,
    }),
  ],
  resolve: {
    alias: {
      '@': path.resolve(__dirname, './src'),
    },
  },
  optimizeDeps: {
    include: ['lowlight'],
    esbuildOptions: { target: 'es2015' },
  },
  server: {
    port: 5193,
    proxy: {
      '/api': {
        target: 'http://localhost:5020',
        changeOrigin: true,
        ws: true,
      }
    }
  }
})
