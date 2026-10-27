// Injected into the page only after the user asks for the panel. It creates a closed shadow root with an extension
// iframe inside. The page cannot read the iframe, and nothing from the page is rendered as HTML.

import { defineContentScript } from 'wxt/utils/define-content-script';
import { browser } from 'wxt/browser';
import { prefilterUrl } from '../src/lib/page-url';
import { isPanelCommand } from '../src/lib/messages';

const HOST_TAG = 'yapspace-root';

interface PanelHandle {
  toggle: () => void;
}

export default defineContentScript({
  matches: ['<all_urls>'],
  registration: 'runtime',
  main(ctx) {
    // A second injection on the same page only toggles the panel that is already there.
    const existing = (window as unknown as { __yapspace?: PanelHandle }).__yapspace;
    if (existing) {
      existing.toggle();
      return;
    }

    let host: HTMLElement | null = null;

    function canonicalHref(): string | null {
      const link = document.querySelector<HTMLLinkElement>('link[rel="canonical" i]');
      const meta = document.querySelector<HTMLMetaElement>('meta[property="og:url"]');
      return link?.href ?? meta?.content ?? null;
    }

    function sendContext() {
      const canonical = canonicalHref();
      void browser.runtime
        .sendMessage({
          type: 'page-context',
          url: prefilterUrl(location.href),
          canonical: canonical ? prefilterUrl(canonical) : null,
          title: document.title.slice(0, 300),
        })
        .catch(() => undefined);
    }

    function mount() {
      const element = document.createElement(HOST_TAG);
      element.setAttribute(
        'style',
        'all: initial; position: fixed; right: 16px; bottom: 16px; width: 380px; ' +
          'height: min(640px, calc(100vh - 32px)); z-index: 2147483647;',
      );
      const shadow = element.attachShadow({ mode: 'closed' });
      const frame = document.createElement('iframe');
      // WXT's typed runtime omits getURL in content scripts, so the one signature needed is spelled out here.
      frame.src = (browser.runtime as unknown as { getURL(path: string): string }).getURL('/panel.html');
      frame.title = 'YapSpace discussion';
      frame.setAttribute(
        'style',
        'border: 0; width: 100%; height: 100%; border-radius: 12px; box-shadow: 0 8px 32px rgba(0,0,0,.25);',
      );
      frame.setAttribute('allow', 'clipboard-write');
      shadow.append(frame);
      document.documentElement.append(element);
      host = element;
      sendContext();
    }

    function unmount() {
      host?.remove();
      host = null;
    }

    const handle: PanelHandle = {
      toggle: () => (host ? unmount() : mount()),
    };
    (window as unknown as { __yapspace?: PanelHandle }).__yapspace = handle;

    browser.runtime.onMessage.addListener((message) => {
      if (isPanelCommand(message)) unmount();
    });

    // Single-page apps change the URL without a page load. Send the new context when that happens.
    ctx.addEventListener(window, 'wxt:locationchange', () => {
      if (host) sendContext();
    });

    mount();
  },
});
