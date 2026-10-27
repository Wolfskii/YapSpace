// Client-side pre-filter for the page URL (docs/architecture/url-identity.md §E.7).
// It is deliberately small. The server does the real normalization and refusal, and it is the only authority on identity.

/**
 * Returns the URL the panel may send to the server, or `null` if the page must not be sent at all.
 *
 * Only http and https pages pass. Credentials and most fragments are removed. Hash routes such as `#/board/1` are
 * kept, because they often name different content.
 */
export function prefilterUrl(href: string): string | null {
  let url: URL;
  try {
    url = new URL(href);
  } catch {
    return null;
  }
  if (url.protocol !== 'http:' && url.protocol !== 'https:') {
    return null;
  }
  if (url.hostname === 'localhost' || isIpLiteral(url.hostname)) {
    return null;
  }
  url.username = '';
  url.password = '';
  if (!(url.hash.startsWith('#/') || url.hash.startsWith('#!/'))) {
    url.hash = '';
  }
  return url.toString();
}

function isIpLiteral(hostname: string): boolean {
  return hostname.startsWith('[') || /^\d{1,3}(\.\d{1,3}){3}$/.test(hostname);
}
