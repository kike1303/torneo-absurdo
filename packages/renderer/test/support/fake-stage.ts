// A fake `Stage` and a fake clock for the player tests (plan-a2 step 2).
import type { SpriteManifest } from '../../src/manifest/index.ts';
import type { RendererOptions, RendererTheme, Stage, StageSetup } from '../../src/player/index.ts';
import type { Scene } from '../../src/scene/index.ts';

export const THEME: RendererTheme = {
  colors: { bg: 1, hp: 2, hpLost: 3, crit: 4, text: 5 },
  fonts: { sans: 'sans-serif', display: 'sans-serif' },
};

export class FakeClock {
  now = 0;
  readonly read = (): number => this.now;
}

export class FakeStage implements Stage {
  setup: StageSetup | null = null;
  scenes: Scene[] = [];
  loaded: ReadonlyMap<string, SpriteManifest> | null = null;
  destroyed = false;
  failInit = false;
  failLoad = false;

  async init(setup: StageSetup): Promise<void> {
    if (this.failInit)
      throw new (await import('../../src/player/index.ts')).RendererError('webgl_unavailable');
    this.setup = setup;
  }
  async loadAssets(manifests: ReadonlyMap<string, SpriteManifest>): Promise<void> {
    if (this.failLoad)
      throw new (await import('../../src/player/index.ts')).RendererError('asset_load_failed');
    this.loaded = manifests;
  }
  apply(scene: Scene): void {
    this.scenes.push(scene);
  }
  setTheme(): void {}
  destroy(): void {
    this.destroyed = true;
  }
  /** One display frame `ms` after the previous one. */
  frame(ms = 16): void {
    this.setup?.onFrame(ms);
  }
  get last(): Scene | undefined {
    return this.scenes.at(-1);
  }
}

export function options(
  clock: FakeClock,
  overrides: Partial<RendererOptions> = {},
): RendererOptions {
  return {
    theme: THEME,
    reducedMotion: false,
    clock: clock.read,
    labels: { names: { a: 'A', b: 'B' }, banners: { fight: 'F', ko: 'K', time: 'T' } },
    formatNumber: String,
    resolveAsset: (id, file) => `/${id}/${file}`,
    ...overrides,
  };
}
