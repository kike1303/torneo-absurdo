// "Copy JSON" of the test arena (TA-20). The Clipboard API only exists in a secure context: on
// the LAN URL of `vite dev` (`http://192.168.x.x:5173`) a phone has no `navigator.clipboard`, so
// the caller shows the text for a manual copy instead.

export type CopyOutcome = 'copied' | 'manual';

/** Copies `text`; `manual` when the API is missing, throws or rejects. Never throws. */
export async function copyText(
  text: string,
  clipboard: Pick<Clipboard, 'writeText'> | undefined,
): Promise<CopyOutcome> {
  if (clipboard === undefined || typeof clipboard.writeText !== 'function') return 'manual';
  try {
    await clipboard.writeText(text);
    return 'copied';
  } catch {
    return 'manual';
  }
}
