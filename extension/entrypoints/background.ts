// The background worker routes page context from content scripts to the panel. It holds no discussion state of its own.
// Browsers may stop a service worker at any time. After a restart the content script sends the context again.

import { defineBackground } from 'wxt/utils/define-background';
import { browser, type Browser } from 'wxt/browser';
import {
  type PageContext,
  type PortMessage,
  isPageContextMessage,
  isPanelCommand,
} from '../src/lib/messages';

const BADGE_MS = 4000;

export default defineBackground(() => {
  const contexts = new Map<number, PageContext>();
  const ports = new Map<number, Browser.runtime.Port>();

  async function openPanel(tabId: number) {
    try {
      // Injected only after a user gesture (the toolbar button or the shortcut), so activeTab covers the page.
      await browser.scripting.executeScript({ target: { tabId }, files: ['content-scripts/panel-host.js'] });
    } catch {
      // chrome://, the web store, PDFs and similar pages cannot be injected. Say so briefly on the toolbar button.
      await browser.action.setBadgeText({ tabId, text: '×' });
      await browser.action.setTitle({ tabId, title: "YapSpace can't run on this page" });
      setTimeout(() => {
        void browser.action.setBadgeText({ tabId, text: '' });
        void browser.action.setTitle({ tabId, title: 'Open YapSpace' });
      }, BADGE_MS);
    }
  }

  browser.action.onClicked.addListener((tab) => {
    if (tab.id !== undefined) void openPanel(tab.id);
  });

  browser.commands.onCommand.addListener(async (command) => {
    if (command !== 'toggle-panel') return;
    const [tab] = await browser.tabs.query({ active: true, currentWindow: true });
    if (tab?.id !== undefined) void openPanel(tab.id);
  });

  browser.runtime.onMessage.addListener((message, sender) => {
    // Only this extension's own pages and scripts may send these. Web pages cannot reach runtime.sendMessage.
    if (sender.id !== browser.runtime.id) return;
    if (isPanelCommand(message)) {
      const tabId = sender.tab?.id;
      if (tabId !== undefined) void browser.tabs.sendMessage(tabId, message).catch(() => undefined);
      return;
    }
    if (!isPageContextMessage(message)) return;
    const tabId = sender.tab?.id;
    if (tabId === undefined) return;
    const context: PageContext = { url: message.url, canonical: message.canonical, title: message.title };
    contexts.set(tabId, context);
    ports.get(tabId)?.postMessage({ type: 'context', context } satisfies PortMessage);
  });

  browser.runtime.onConnect.addListener((port) => {
    if (port.name !== 'panel' || port.sender?.id !== browser.runtime.id) return;
    const tabId = port.sender?.tab?.id;
    if (tabId === undefined) return;
    ports.set(tabId, port);
    const context = contexts.get(tabId);
    if (context) port.postMessage({ type: 'context', context } satisfies PortMessage);
    port.onDisconnect.addListener(() => {
      if (ports.get(tabId) === port) ports.delete(tabId);
    });
  });

  browser.tabs.onRemoved.addListener((tabId) => {
    contexts.delete(tabId);
    ports.delete(tabId);
  });
});
