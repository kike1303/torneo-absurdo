import { Loader, type LoaderParser } from 'pixi.js';
import { describe, expect, it } from 'vitest';

// TA-40: the shared cache loads again after a failed unload. That is safe only while `Loader.unload`
// forgets the URL before it calls `parser.unload`: otherwise `Assets.load` would hand back the
// half-destroyed asset. A data URL keeps `path.toAbsolute` away from `document` in Node.
const ASSET = 'data:text/plain,contract';

describe('Pixi Loader.unload', () => {
  it('forgets the URL before the parser unloads and rejects with the parser error', async () => {
    const failure = new Error('unload failed');
    const loader = new Loader();
    const entriesDuringUnload: unknown[] = [];
    const parser: LoaderParser = {
      id: 'contract',
      test: () => true,
      load: async () => ({}),
      unload: () => {
        entriesDuringUnload.push(loader.promiseCache[ASSET]);
        throw failure;
      },
    };
    loader.parsers.push(parser);

    const first = await loader.load<object>(ASSET);
    expect(loader.promiseCache[ASSET]).toBeDefined();

    await expect(loader.unload(ASSET)).rejects.toBe(failure);
    expect(entriesDuringUnload).toEqual([undefined]);
    expect(loader.promiseCache[ASSET]).toBeUndefined();
    expect(await loader.load<object>(ASSET)).not.toBe(first);
  });
});
