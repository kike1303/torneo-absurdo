import { describe, expect, it, vi } from 'vitest';
import { copyText } from './copy-text.js';

// TA-20: on an insecure context (the LAN URL of `vite dev` on a phone) `navigator.clipboard` is
// undefined; the arena must fall back to a manual copy instead of failing silently.
describe('copyText', () => {
  it('copies with the Clipboard API when it is there', async () => {
    const writeText = vi.fn(() => Promise.resolve());
    await expect(copyText('{"a":1}', { writeText })).resolves.toBe('copied');
    expect(writeText).toHaveBeenCalledWith('{"a":1}');
  });

  it('asks for a manual copy when the Clipboard API is undefined', async () => {
    await expect(copyText('{}', undefined)).resolves.toBe('manual');
  });

  it('asks for a manual copy when writeText rejects', async () => {
    const writeText = () => Promise.reject(new DOMException('denied', 'NotAllowedError'));
    await expect(copyText('{}', { writeText })).resolves.toBe('manual');
  });

  it('asks for a manual copy when writeText throws synchronously', async () => {
    const writeText = (): Promise<void> => {
      throw new TypeError('not a function');
    };
    await expect(copyText('{}', { writeText })).resolves.toBe('manual');
  });
});
