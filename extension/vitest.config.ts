import { svelte } from '@sveltejs/vite-plugin-svelte';
import { defineConfig } from 'vitest/config';

export default defineConfig({
  plugins: [svelte({ hot: false })],
  resolve: {
    // Svelte's browser build is needed so components and stores mount in jsdom.
    conditions: ['browser'],
  },
  define: {
    __API_BASE__: JSON.stringify('http://api.test'),
    __WS_URL__: JSON.stringify('ws://api.test/v1/realtime'),
  },
  test: {
    include: ['tests/**/*.test.ts'],
    environment: 'jsdom',
  },
});
