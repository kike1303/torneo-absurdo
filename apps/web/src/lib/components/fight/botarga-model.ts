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

/** What one side wears, by id: the host's way to ask for a `FighterLook`. */
export interface LookSpec {
  /** Folder of content/models/skins/; without it, the base botarga. */
  readonly skin?: string;
  /** Ids of content/models/accessories/ (the ones of the catalog). */
  readonly wear?: readonly string[];
  /** `#RRGGBB` by color slot of the accessories (`primary`…) or by zone id of the skin (`z1`…). */
  readonly colors?: Readonly<Record<string, string>>;
}
export type LookSpecs = Readonly<Partial<Record<Side, LookSpec>>>;

const TINT_SLOT_NAMES: readonly TintSlot[] = ['primary', 'secondary', 'accent'];

function fileOf<T>(files: Record<string, () => Promise<T>>, folder: string, name?: string) {
  const prefix = `/${folder}/`;
  return Object.entries(files).find(
    ([path]) => path.includes(prefix) && (name === undefined || path.endsWith(`/${name}`)),
  )?.[1];
}

/**
 * The manifest readers of the renderer. FightStage passes them from its `import()`: nothing else
 * imports values from the renderer (ADR-0009 D4).
 */
export type Parsers = Pick<
  typeof import('@torneo/renderer/three'),
  'parseAccessoryManifest' | 'parseSkinManifest'
>;

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
      const manifest = fileOf(accessoryManifests, `accessories/${id}`);
      const glb = fileOf(accessoryGlbs, `accessories/${id}`, `${id}.glb`);
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
 * Puts the GLB and the faces in the HTTP cache while the events of a live fight come, as the
 * sheets do in 2D (010 plan-d decision 14). `<link rel="preload">`: the loaders then fetch them
 * from the cache. A failure here is not one: the load asks for them again and reports it.
 */
export function preloadBotargaModel(): void {
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
}
