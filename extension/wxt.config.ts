import { defineConfig } from 'wxt';

// The API origin is configured at build time. Only that origin is granted as a host permission.
const env = (globalThis as { process?: { env?: Record<string, string | undefined> } }).process?.env ?? {};
const apiBase = env.WXT_API_BASE_URL ?? 'http://localhost:8080';
const apiOrigin = new URL(apiBase).origin;

export default defineConfig({
  modules: ['@wxt-dev/module-svelte'],
  manifest: ({ browser }) => ({
    name: 'YapSpace',
    description: 'The internet is better when we talk.',
    default_locale: 'en',
    // activeTab plus scripting only inject the panel after the user clicks. There is no <all_urls> permission.
    permissions: ['activeTab', 'scripting', 'storage'],
    host_permissions: [`${apiOrigin}/*`],
    action: { default_title: 'Open YapSpace' },
    commands: {
      'toggle-panel': {
        suggested_key: { default: 'Alt+Y' },
        description: 'Toggle YapSpace on this page',
      },
    },
    web_accessible_resources: [
      {
        resources: ['panel.html'],
        matches: ['<all_urls>'],
        ...(browser === 'firefox' ? {} : { use_dynamic_url: true }),
      },
    ],
    ...(browser === 'firefox'
      ? { browser_specific_settings: { gecko: { id: 'extension@yapspace.app', strict_min_version: '128.0' } } }
      : {}),
  }),
  vite: () => ({
    define: {
      __API_BASE__: JSON.stringify(apiBase),
      __WS_URL__: JSON.stringify(apiBase.replace(/^http/, 'ws') + '/v1/realtime'),
    },
  }),
});
