import dummyA from '../../placeholders/dummy_a/manifest.json' with { type: 'json' };
import dummyB from '../../placeholders/dummy_b/manifest.json' with { type: 'json' };
import { validateManifest, type SpriteManifest } from '../../src/manifest/index.ts';

function load(json: unknown, id: string): SpriteManifest {
  const result = validateManifest(json, { expectedId: id });
  if (!result.ok) throw new Error(JSON.stringify(result.errors));
  return result.manifest;
}

export const DUMMY_A = load(dummyA, 'dummy_a');
export const DUMMY_B = load(dummyB, 'dummy_b');
export const MANIFESTS: ReadonlyMap<string, SpriteManifest> = new Map([
  ['dummy_a', DUMMY_A],
  ['dummy_b', DUMMY_B],
]);
