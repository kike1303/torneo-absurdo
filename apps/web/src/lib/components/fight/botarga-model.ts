// The generic botarga of the 3D arena (013): its GLB, its faces and the tint of each side, all in
// content/models/botarga-base/ (own art, all rights reserved). Vite hashes the files; nothing here
// is fetched until `loadBotargaModel` runs, on mount, next to the `import()` of the renderer.
import type { BotargaModel, FaceCode } from '@torneo/renderer/three';

const glb = () => import('../../../../../../content/models/botarga-base/botarga-base.glb?url');
const tints = () => import('../../../../../../content/models/botarga-base/tints.json');
const faces = import.meta.glob<string>(
  '../../../../../../content/models/botarga-base/faces/*.png',
  {
    query: '?url',
    import: 'default',
  },
);

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

export async function loadBotargaModel(): Promise<BotargaModel> {
  const faceEntries = await Promise.all(
    USED_FACES.map(async (code) => {
      const load = Object.entries(faces).find(([path]) => path.endsWith(`/${code}.png`))?.[1];
      return [code, load === undefined ? undefined : await load()] as const;
    }),
  );
  const [glbModule, tintsModule] = await Promise.all([glb(), tints()]);
  return {
    glb: glbModule.default,
    faces: Object.fromEntries(faceEntries.filter(([, url]) => url !== undefined)),
    tints: tintsModule.default,
  };
}
