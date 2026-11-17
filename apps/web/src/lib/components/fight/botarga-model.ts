// The generic botarga of the 3D arena (013): its GLB, its faces and the tint of each side, all in
// content/models/botarga-base/ (own art, all rights reserved). Vite hashes the files; nothing here
// is fetched until `loadBotargaModel` runs, on mount, next to the `import()` of the renderer.
import type { Side } from '@torneo/renderer';
import type {
  AccessoryModel,
  BotargaModel,
  FaceCode,
  FighterLook,
  SkinModel,
  TintSlot,
} from '@torneo/renderer/three';

const glb = () => import('../../../../../../content/models/botarga-base/botarga-base.glb?url');
const tints = () => import('../../../../../../content/models/botarga-base/tints.json');
const faces = import.meta.glob<string>(
  '../../../../../../content/models/botarga-base/faces/*.png',
  {
    query: '?url',
    import: 'default',
  },
);

// The wardrobe (022): accessories by catalog id and skins by slug, each in its folder of
// content/models/. Lazy as the rest: a file is fetched only when a fighter wears it.
const accessoryManifests = import.meta.glob<unknown>(
  '../../../../../../content/models/accessories/*/accessory.json',
  { import: 'default' },
);
const accessoryGlbs = import.meta.glob<string>(
  '../../../../../../content/models/accessories/*/*.glb',
  { query: '?url', import: 'default' },
);
// The house uniform (TA-91): accessories in the same format, out of the catalog, that only the
// fighters of the house fights wear. A player never gets them: the server checks the catalog.
const uniformManifests = import.meta.glob<unknown>(
  '../../../../../../content/models/uniform/*/accessory.json',
  { import: 'default' },
);
const uniformGlbs = import.meta.glob<string>('../../../../../../content/models/uniform/*/*.glb', {
  query: '?url',
  import: 'default',
});
const skinManifests = import.meta.glob<unknown>(
  '../../../../../../content/models/skins/*/skin.json',
  { import: 'default' },
);
const skinGlbs = import.meta.glob<string>('../../../../../../content/models/skins/*/*.glb', {
  query: '?url',
  import: 'default',
});
const skinImages = import.meta.glob<string>('../../../../../../content/models/skins/*/*.png', {
  query: '?url',
  import: 'default',
});

/** Folder names under `kind/` of the glob keys, sorted: what exists, without fetching a file. */
function folders(files: Record<string, unknown>, kind: string): string[] {
  const names = Object.keys(files).flatMap((path) => {
    const name = new RegExp(`/content/models/${kind}/([^/]+)/`).exec(path)?.[1];
    return name === undefined ? [] : [name];
  });
  return [...new Set(names)].sort();
}

/** The wardrobe that exists in content/models/ (TA-76): accessory ids and skin slugs. */
export interface WardrobeCatalog {
  readonly accessories: readonly string[];
  readonly skins: readonly string[];
}

/** An accessory or skin needs both its manifest and its GLB to be worn, so only those count. */
export function wardrobeCatalog(): WardrobeCatalog {
  const both = (
    manifests: Record<string, unknown>,
    glbs: Record<string, unknown>,
    kind: string,
  ) => {
    const withGlb = new Set(folders(glbs, kind));
    return folders(manifests, kind).filter((id) => withGlb.has(id));
  };
  return {
    accessories: both(accessoryManifests, accessoryGlbs, 'accessories'),
    // A skin's manifest is optional (`loadLook`): its GLB is enough.
    skins: folders(skinGlbs, 'skins'),
  };
}

/** The pieces of the house uniform, in content/models/uniform/. */
export const HOUSE_UNIFORM: readonly string[] = ['tournament_shirt', 'tournament_pants'];

/** What both fighters of a house fight wear (TA-91). */
export const HOUSE_LOOKS: LookSpecs = { a: { wear: HOUSE_UNIFORM }, b: { wear: HOUSE_UNIFORM } };

/** What one side wears, by id: the host's way to ask for a `FighterLook`. */
export interface LookSpec {
  /** Folder of content/models/skins/; without it, the base botarga. */
  readonly skin?: string;
  /** Ids of content/models/accessories/ (the ones of the catalog) or of content/models/uniform/. */
  readonly wear?: readonly string[];
  /** `#RRGGBB` by color slot of the accessories (`primary`…) or by zone id of the skin (`z1`…). */
  readonly colors?: Readonly<Record<string, string>>;
}
export type LookSpecs = Readonly<Partial<Record<Side, LookSpec>>>;

const TINT_SLOT_NAMES: readonly TintSlot[] = ['primary', 'secondary', 'accent'];

/** The file of `folder` (`<kind>/<id>`), by its exact two last directories: never by a substring. */
function fileOf<T>(files: Record<string, () => Promise<T>>, folder: string, name?: string) {
  return Object.entries(files).find(([path]) => {
    const parts = path.split('/');
    return (
      parts.slice(-3, -1).join('/') === folder && (name === undefined || parts.at(-1) === name)
    );
  })?.[1];
}

/**
 * The manifest readers of the renderer. FightStage passes them from its `import()`: nothing else
 * imports values from the renderer (ADR-0009 D4).
 */
export type Parsers = Pick<
  typeof import('@torneo/renderer/three'),
  'parseAccessoryManifest' | 'parseSkinManifest'
>;

/** The GLB of an accessory, or of a piece of the house uniform, by its id. */
function accessoryGlb(id: string): (() => Promise<string>) | undefined {
  return (
    fileOf(accessoryGlbs, `accessories/${id}`, `${id}.glb`) ??
    fileOf(uniformGlbs, `uniform/${id}`, `${id}.glb`)
  );
}

/** An id that is not in content/models/, or with a broken manifest, is skipped: nothing is worn. */
async function loadLook(spec: LookSpec, parsers: () => Promise<Parsers>): Promise<FighterLook> {
  const { parseAccessoryManifest, parseSkinManifest } = await parsers();
  const slotColors = Object.fromEntries(
    TINT_SLOT_NAMES.flatMap((slot) => {
      const color = spec.colors?.[slot];
      return color === undefined ? [] : [[slot, color]];
    }),
  );
  const accessories = await Promise.all(
    [...new Set(spec.wear ?? [])].map(async (id): Promise<AccessoryModel | null> => {
      const manifest =
        fileOf(accessoryManifests, `accessories/${id}`) ??
        fileOf(uniformManifests, `uniform/${id}`);
      const glb = accessoryGlb(id);
      if (manifest === undefined || glb === undefined) return null;
      const parsed = parseAccessoryManifest(await manifest());
      return parsed === null ? null : { glb: await glb(), manifest: parsed, colors: slotColors };
    }),
  );
  let skin: SkinModel | undefined;
  if (spec.skin !== undefined) {
    const folder = `skins/${spec.skin}`;
    const glb = fileOf(skinGlbs, folder, `${spec.skin}.glb`);
    const manifest = fileOf(skinManifests, folder);
    const parsed = manifest === undefined ? {} : parseSkinManifest(await manifest());
    if (glb !== undefined && parsed !== null) {
      const map =
        parsed.islandMap === undefined
          ? undefined
          : fileOf(skinImages, folder, parsed.islandMap.file);
      skin = {
        glb: await glb(),
        fit: parsed.fit,
        zones: parsed.zones,
        ...(parsed.islandMap === undefined || map === undefined
          ? {}
          : {
              islandMap: {
                url: await map(),
                material: parsed.islandMap.material,
                step: parsed.islandMap.step,
              },
            }),
      };
    }
  }
  return {
    skin,
    colors: spec.colors,
    accessories: accessories.filter((accessory) => accessory !== null),
  };
}

/** The faces the pose uses (packages/renderer/src/model/pose.ts); the rest are not downloaded. */
const USED_FACES: readonly FaceCode[] = [
  'neutral',
  'guardia',
  'preparacion',
  'golpe',
  'enfadado',
  'dano',
  'concentrado',
  'alerta',
  'ko',
  'victoria',
  'cansado',
];

export async function loadBotargaModel(
  looks?: LookSpecs,
  parsers?: () => Promise<Parsers>,
): Promise<BotargaModel> {
  const faceEntries = await Promise.all(
    USED_FACES.map(async (code) => {
      const load = Object.entries(faces).find(([path]) => path.endsWith(`/${code}.png`))?.[1];
      return [code, load === undefined ? undefined : await load()] as const;
    }),
  );
  const [glbModule, tintsModule, a, b] = await Promise.all([
    glb(),
    tints(),
    looks?.a === undefined || parsers === undefined ? undefined : loadLook(looks.a, parsers),
    looks?.b === undefined || parsers === undefined ? undefined : loadLook(looks.b, parsers),
  ]);
  return {
    glb: glbModule.default,
    faces: Object.fromEntries(faceEntries.filter(([, url]) => url !== undefined)),
    tints: tintsModule.default,
    ...(a === undefined && b === undefined ? {} : { looks: { a, b } }),
  };
}

/**
 * Puts the GLB, the faces and what the fighters wear in the HTTP cache while the events of a live
 * fight come, as the sheets do in 2D (010 plan-d decision 14). `<link rel="preload">`: the loaders
 * then fetch them from the cache. A failure here is not one: the load asks for them again and reports it.
 */
export function preloadBotargaModel(looks?: LookSpecs): void {
  const link = (href: string, as: 'fetch' | 'image'): void => {
    if (document.head.querySelector(`link[rel="preload"][href="${CSS.escape(href)}"]`)) return;
    const el = document.createElement('link');
    el.rel = 'preload';
    el.as = as;
    el.href = href;
    // Three loads both in CORS mode (`anonymous`); a preload in another mode would not be reused.
    el.crossOrigin = 'anonymous';
    document.head.appendChild(el);
  };
  void loadBotargaModel()
    .then((model) => {
      link(model.glb, 'fetch');
      for (const url of Object.values(model.faces)) if (url !== undefined) link(url, 'image');
    })
    .catch(() => undefined);
  // What they wear (the house uniform in every house fight), so it does not wait for the renderer.
  const worn = new Set([...(looks?.a?.wear ?? []), ...(looks?.b?.wear ?? [])]);
  for (const id of worn) {
    void accessoryGlb(id)?.()
      .then((url) => link(url, 'fetch'))
      .catch(() => undefined);
  }
}
