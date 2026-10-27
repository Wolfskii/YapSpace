// Turns message text into safe tokens. Only http and https links become links, and everything else is text. Nothing
// is ever inserted as HTML, so `javascript:` links and markup in messages stay plain text.

export type Token = { type: 'text'; value: string } | { type: 'link'; value: string; href: string };

const URL_PATTERN = /https?:\/\/[^\s<>"']+/g;
const TRAILING_PUNCTUATION = /[.,!?;:)\]]+$/;

export function linkify(text: string): Token[] {
  const tokens: Token[] = [];
  let last = 0;
  for (const match of text.matchAll(URL_PATTERN)) {
    const start = match.index ?? 0;
    // Trailing punctuation is usually sentence punctuation, not part of the link.
    const raw = match[0].replace(TRAILING_PUNCTUATION, '');
    const href = safeHttpUrl(raw);
    if (href === null) continue;
    if (start > last) tokens.push({ type: 'text', value: text.slice(last, start) });
    tokens.push({ type: 'link', value: raw, href });
    last = start + raw.length;
  }
  if (last < text.length) tokens.push({ type: 'text', value: text.slice(last) });
  return tokens;
}

function safeHttpUrl(candidate: string): string | null {
  try {
    const url = new URL(candidate);
    return url.protocol === 'http:' || url.protocol === 'https:' ? url.toString() : null;
  } catch {
    return null;
  }
}
