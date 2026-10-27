// Messages between the content script, the background and the panel. Every value that crosses a boundary is checked
// with a type guard before use, because the page can never be trusted.

export interface PageContext {
  /** Pre-filtered URL, or null when the page must not be discussed. */
  url: string | null;
  /** The page's canonical URL, if it declares one. */
  canonical: string | null;
  title: string;
}

/** Content script to background. */
export interface PageContextMessage extends PageContext {
  type: 'page-context';
}

/** Background to panel, over a long-lived port. */
export type PortMessage = { type: 'context'; context: PageContext } | { type: 'close' };

/** Background to content script. */
export type PanelCommand = { type: 'panel-close' };

function isObject(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}

function isNullableString(value: unknown): value is string | null {
  return value === null || typeof value === 'string';
}

export function isPageContextMessage(value: unknown): value is PageContextMessage {
  return (
    isObject(value) &&
    value.type === 'page-context' &&
    isNullableString(value.url) &&
    isNullableString(value.canonical) &&
    typeof value.title === 'string'
  );
}

export function isPortMessage(value: unknown): value is PortMessage {
  if (!isObject(value)) return false;
  if (value.type === 'close') return true;
  return value.type === 'context' && isObject(value.context) && isNullableString(value.context.url);
}

export function isPanelCommand(value: unknown): value is PanelCommand {
  return isObject(value) && value.type === 'panel-close';
}
