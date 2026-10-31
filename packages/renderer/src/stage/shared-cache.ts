// Reference counts over a global loader (QA F1 and F2 of A2). Pixi's `Assets` cache is global:
// two renderers that overlap (a fighter switched quickly, a retry) get the same texture for the
// same URL, so an `Assets.unload` from the old one would destroy what the new one draws. Each URL
// is loaded once, unloaded only after its last owner releases it, and a new acquire waits for an
// unload in flight. A failed unload does not fail that acquire: Pixi forgets the URL before it
// destroys the asset, so the load starts clean. A load that failed is forgotten, never unloaded.

export interface SharedCache<T> {
  acquire(url: string): Promise<T>;
  release(url: string): Promise<void>;
}

interface Entry<T> {
  count: number;
  readonly promise: Promise<T>;
}

export function createSharedCache<T>(loader: {
  load: (url: string) => Promise<T>;
  unload: (url: string) => Promise<void>;
}): SharedCache<T> {
  const entries = new Map<string, Entry<T>>();
  const unloading = new Map<string, Promise<void>>();

  return {
    acquire(url) {
      const existing = entries.get(url);
      if (existing !== undefined) {
        existing.count++;
        return existing.promise;
      }
      const pending = unloading.get(url) ?? Promise.resolve();
      const load = () => loader.load(url);
      const promise = pending.then(load, load);
      const entry: Entry<T> = { count: 1, promise };
      entries.set(url, entry);
      promise.catch(() => {
        if (entries.get(url) === entry) entries.delete(url);
      });
      return promise;
    },
    async release(url) {
      const entry = entries.get(url);
      if (entry === undefined) return;
      entry.count--;
      if (entry.count > 0) return;
      entries.delete(url);
      const done = entry.promise.then(
        () => loader.unload(url),
        () => undefined,
      );
      unloading.set(url, done);
      try {
        await done;
      } finally {
        if (unloading.get(url) === done) unloading.delete(url);
      }
    },
  };
}
