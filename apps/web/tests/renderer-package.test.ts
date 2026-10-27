import { describe, expect, it } from 'vitest';
import { validateManifest } from '@torneo/renderer/manifest';
import { validateTimeline } from '@torneo/renderer/timeline';
import dummyA from '@torneo/renderer/placeholders/dummy_a/manifest.json';
import dummyB from '@torneo/renderer/placeholders/dummy_b/manifest.json';
import spike from '@torneo/renderer/fixtures/spike.timeline.json';

// RF-A1 (002 plan-a1, step 9): Vite/Vitest and svelte-check resolve the subpaths of
// @torneo/renderer and its `.ts` relative imports with the tsconfig of apps/web.
describe('@torneo/renderer from apps/web', () => {
  it('validates the placeholder manifests and the spike timeline', () => {
    const a = validateManifest(dummyA, { expectedId: 'dummy_a' });
    const b = validateManifest(dummyB, { expectedId: 'dummy_b' });
    expect(a.ok && b.ok).toBe(true);
    if (!a.ok || !b.ok) return;
    const manifests = new Map([
      ['dummy_a', a.manifest],
      ['dummy_b', b.manifest],
    ]);
    expect(validateTimeline(spike, manifests).ok).toBe(true);
  });
});
