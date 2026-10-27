import { describe, expect, it } from 'vitest';
import { prefilterUrl } from '../src/lib/page-url';
import { linkify } from '../src/lib/linkify';
import { isPageContextMessage, isPanelCommand, isPortMessage } from '../src/lib/messages';

describe('prefilterUrl (client side, url-identity.md §E.7)', () => {
  it.each([
    ['https://example.com/a?x=1', 'https://example.com/a?x=1'],
    ['http://example.com/', 'http://example.com/'],
    ['https://user:secret@example.com/a', 'https://example.com/a'],
    ['https://example.com/a#section-2', 'https://example.com/a'],
    ['https://app.example.com/#/board/42', 'https://app.example.com/#/board/42'],
    ['https://app.example.com/#!/inbox', 'https://app.example.com/#!/inbox'],
    ['https://example.com/cb#access_token=abc', 'https://example.com/cb'],
  ])('keeps the right parts of %s', (input, expected) => {
    expect(prefilterUrl(input)).toBe(expected);
  });

  it.each([
    'file:///C:/x.html',
    'chrome://extensions',
    'about:blank',
    'javascript:alert(1)',
    'http://localhost:3000/',
    'http://192.168.1.1/',
    'http://[::1]/',
    'not a url',
  ])('refuses %s', (input) => {
    expect(prefilterUrl(input)).toBeNull();
  });
});

describe('linkify (extension.md §I.4, security.md §H.3)', () => {
  it('turns http and https links into link tokens and keeps the rest as text', () => {
    expect(linkify('see https://example.com/a, thanks')).toEqual([
      { type: 'text', value: 'see ' },
      { type: 'link', value: 'https://example.com/a', href: 'https://example.com/a' },
      { type: 'text', value: ', thanks' },
    ]);
  });

  it('never turns javascript: or data: text into a link', () => {
    const tokens = linkify('javascript:alert(1) data:text/html,<b>x</b>');
    expect(tokens.every((token) => token.type === 'text')).toBe(true);
  });

  it('strips trailing sentence punctuation from links', () => {
    const [, link] = linkify('go to https://example.com/x.');
    expect(link).toMatchObject({ type: 'link', value: 'https://example.com/x' });
  });

  it('keeps markup as plain text', () => {
    expect(linkify('<img src=x onerror=alert(1)>')).toEqual([{ type: 'text', value: '<img src=x onerror=alert(1)>' }]);
  });
});

describe('message guards (extension.md §I.3)', () => {
  it('accepts a well-formed page context and rejects anything else', () => {
    expect(isPageContextMessage({ type: 'page-context', url: null, canonical: null, title: '' })).toBe(true);
    expect(isPageContextMessage({ type: 'page-context', url: 5, canonical: null, title: '' })).toBe(false);
    expect(isPageContextMessage({ type: 'other' })).toBe(false);
    expect(isPageContextMessage(null)).toBe(false);
  });

  it('checks port and panel messages', () => {
    expect(isPortMessage({ type: 'close' })).toBe(true);
    expect(isPortMessage({ type: 'context', context: { url: 'https://a.test/', canonical: null, title: '' } })).toBe(true);
    expect(isPortMessage({ type: 'context' })).toBe(false);
    expect(isPanelCommand({ type: 'panel-close' })).toBe(true);
    expect(isPanelCommand({ type: 'panel-open' })).toBe(false);
  });
});
