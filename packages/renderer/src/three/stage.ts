// The Three.js layer (013): the same `Stage` as the Pixi one, with two botargas instead of sprites.
// The sprite manifests still give the timing of each animation (the player validates them), but
// their sheets are never loaded here.
import {
  Color,
  DirectionalLight,
  DynamicDrawUsage,
  HemisphereLight,
  InstancedMesh,
  Matrix4,
  Mesh,
  MeshBasicMaterial,
  MeshStandardMaterial,
  NeutralToneMapping,
  OctahedronGeometry,
  CircleGeometry,
  DataTexture,
  SRGBColorSpace,
  Scene as ThreeScene,
  WebGLRenderer,
} from 'three';
import { MAX_PARTICLES } from '../fx/particles.ts';
import { poseFor } from '../model/pose.ts';
import { RendererError } from '../player/errors.ts';
import type { RendererTheme, Stage, StageSetup } from '../player/types.ts';
import type { DamageNumberEffect, Scene } from '../scene/types.ts';
import { SIDES, type Side } from '../timeline/types.ts';
import { createFighter, type Fighter3d } from './fighter.ts';
import { loadBotarga, type Botarga } from './load.ts';
import { createOverlay, type Overlay } from './overlay.ts';
import type { BotargaModel } from './types.ts';
import { METERS_PER_PX, createCamera, placeCamera, worldX, worldY } from './world.ts';

/** Device pixels per CSS px, capped: a phone at 3× costs more than it shows. */
const MAX_PIXEL_RATIO = 2;
/**
 * A device that cannot keep up draws fewer pixels: when the smoothed frame interval passes
 * `SLOW_FRAME_MS`, the pixel ratio steps down by `RATIO_STEP` (at most once per
 * `RATIO_COOLDOWN_MS`), never under `MIN_PIXEL_RATIO`. Gaps over `IGNORED_GAP_MS` (a hidden tab)
 * do not count.
 */
const SLOW_FRAME_MS = 40;
const RATIO_STEP = 0.75;
const MIN_PIXEL_RATIO = 0.5;
const RATIO_COOLDOWN_MS = 1000;
const IGNORED_GAP_MS = 250;
const FRAME_SMOOTHING = 0.1;
/**
 * A soft disc under each fighter instead of a shadow map: no shadow pass and simpler shaders, which
 * a weak GPU (or the software WebGL of the e2e) feels most on the first frame.
 */
const BLOB_RADIUS = 0.32;
const BLOB_TEXELS = 64;
/** How much darker than the floor the blob is. */
const BLOB_SHADE = 0.55;
/** Just above the floor, so they never fight for depth. */
const BLOB_LIFT = 0.002;
/** How much of the text color the floor takes over the background. */
const FLOOR_MIX = 0.08;
const FLOOR_RADIUS = 3;
/** Particles are drawn larger than their logical size: in 3D they sit behind perspective. */
const PARTICLE_SCALE = 3;
/** Particles sit a little in front of the fighters, towards the camera. */
const PARTICLE_DEPTH = 0.3;

interface World {
  readonly renderer: WebGLRenderer;
  readonly scene: ThreeScene;
  readonly camera: ReturnType<typeof createCamera>;
  readonly floor: Mesh<CircleGeometry, MeshStandardMaterial>;
  readonly hemisphere: HemisphereLight;
  readonly particles: InstancedMesh<OctahedronGeometry, MeshBasicMaterial>;
  readonly blobs: Record<Side, Mesh<CircleGeometry, MeshBasicMaterial>>;
  readonly overlay: Overlay;
}

/** Software rasterizers by the name they report (Chrome without a GPU, Mesa, Windows). */
const SOFTWARE_RENDERERS = /swiftshader|llvmpipe|softpipe|basic render driver/i;

/**
 * Whether WebGL runs here on the CPU (a software rasterizer, as on a machine without a usable GPU):
 * then it starts at the cheapest settings instead of finding them one slow frame at a time.
 */
function softwareOnly(): boolean {
  const gl = document
    .createElement('canvas')
    .getContext('webgl2', { failIfMajorPerformanceCaveat: true });
  if (gl === null) return true;
  const info = gl.getExtension('WEBGL_debug_renderer_info');
  const name: unknown = info === null ? '' : gl.getParameter(info.UNMASKED_RENDERER_WEBGL);
  gl.getExtension('WEBGL_lose_context')?.loseContext();
  return typeof name === 'string' && SOFTWARE_RENDERERS.test(name);
}

/** Alpha falls from the center to the rim (a smoothstep), so the blob has no edge. */
function blobAlpha(): DataTexture {
  const data = new Uint8Array(BLOB_TEXELS * BLOB_TEXELS * 4);
  const half = BLOB_TEXELS / 2;
  for (let y = 0; y < BLOB_TEXELS; y++) {
    for (let x = 0; x < BLOB_TEXELS; x++) {
      const r = Math.min(1, Math.hypot(x + 0.5 - half, y + 0.5 - half) / half);
      const fall = 1 - r * r * (3 - 2 * r);
      const i = (y * BLOB_TEXELS + x) * 4;
      data.fill(Math.round(fall * 255), i, i + 4);
    }
  }
  const texture = new DataTexture(data, BLOB_TEXELS, BLOB_TEXELS);
  texture.needsUpdate = true;
  return texture;
}

export function createThreeStage(container: HTMLElement, model: BotargaModel): Stage {
  let world: World | null = null;
  let theme: RendererTheme | null = null;
  let botarga: Botarga | null = null;
  let fighters: Record<Side, Fighter3d> | null = null;
  let lastScene: Scene | null = null;
  let observer: ResizeObserver | null = null;
  let destroyed = false;
  let aspect = 16 / 9;
  /** Software WebGL (see `softwareOnly`): cheapest settings and diffuse-only botargas. */
  let lite = false;
  /** Something changed since the last render: a still scene (paused, hidden) costs nothing. */
  let dirty = true;
  let formatNumber: (n: number) => string = String;
  const numbers: DamageNumberEffect[] = [];
  const matrix = new Matrix4();
  const tint = new Color();

  const paint = (w: World, current: RendererTheme): void => {
    const bg = new Color(current.colors.bg);
    w.scene.background = bg;
    w.floor.material.color.copy(bg).lerp(new Color(current.colors.text), FLOOR_MIX);
    w.hemisphere.groundColor.copy(bg);
    for (const side of SIDES)
      w.blobs[side].material.color.copy(w.floor.material.color).multiplyScalar(BLOB_SHADE);
  };

  const resize = (): void => {
    const w = world;
    if (w === null) return;
    const width = Math.max(1, container.clientWidth);
    const height = Math.max(1, container.clientHeight);
    w.renderer.setSize(width, height, false);
    aspect = width / height;
    w.overlay.layout({ width, height });
    if (lastScene !== null) draw(w, lastScene);
    dirty = true;
  };

  function drawParticles(w: World, scene: Scene, current: RendererTheme): void {
    let count = 0;
    for (const effect of scene.effects) {
      if (effect.kind !== 'particle' || count >= MAX_PARTICLES) continue;
      const size = effect.size * METERS_PER_PX * PARTICLE_SCALE * effect.alpha;
      matrix.makeScale(size, size, size);
      matrix.setPosition(worldX(effect.x), worldY(effect.y), PARTICLE_DEPTH);
      w.particles.setMatrixAt(count, matrix);
      w.particles.setColorAt(count, tint.setHex(current.colors[effect.color]));
      count++;
    }
    w.particles.count = count;
    w.particles.instanceMatrix.needsUpdate = true;
    if (w.particles.instanceColor !== null) w.particles.instanceColor.needsUpdate = true;
  }

  function draw(w: World, scene: Scene): void {
    const current = theme;
    if (current === null) return;
    placeCamera(w.camera, aspect, scene.camera);
    for (const side of SIDES) w.blobs[side].position.x = worldX(scene.fighters[side].x);
    if (fighters !== null && botarga !== null) {
      for (const side of SIDES) {
        const f = scene.fighters[side];
        const hud = scene.hud[side];
        let flash = 0;
        for (const effect of scene.effects) {
          if (effect.kind === 'flash' && effect.side === side)
            flash = Math.max(flash, effect.alpha);
        }
        const pose = poseFor(f, scene.t, botarga.durations, {
          hpFraction: hud.maxHp > 0 ? hud.hp / hud.maxHp : 1,
          intro: scene.phase === 'intro',
        });
        fighters[side].apply(f, pose, scene.t, flash);
      }
    }
    drawParticles(w, scene, current);
    numbers.length = 0;
    for (const effect of scene.effects) if (effect.kind === 'damage') numbers.push(effect);
    w.overlay.apply(scene.hud, scene.banners, numbers, w.camera, formatNumber);
  }

  const disposeFighters = (): void => {
    if (fighters === null) return;
    for (const side of SIDES) {
      world?.scene.remove(fighters[side].root);
      fighters[side].dispose();
    }
    fighters = null;
  };

  return {
    async init(setup: StageSetup) {
      theme = setup.theme;
      formatNumber = setup.formatNumber;
      const software = softwareOnly();
      lite = software;
      let renderer: WebGLRenderer;
      try {
        // At 2× and above the pixels are small enough without MSAA, which is the costly part.
        renderer = new WebGLRenderer({
          antialias: !software && (window.devicePixelRatio || 1) < MAX_PIXEL_RATIO,
          powerPreference: 'high-performance',
        });
      } catch (cause) {
        throw new RendererError('webgl_unavailable', { cause });
      }
      if (destroyed) {
        renderer.dispose();
        renderer.forceContextLoss();
        return;
      }
      let pixelRatio = software
        ? MIN_PIXEL_RATIO
        : Math.min(window.devicePixelRatio || 1, MAX_PIXEL_RATIO);
      renderer.setPixelRatio(pixelRatio);
      renderer.outputColorSpace = SRGBColorSpace;
      renderer.toneMapping = NeutralToneMapping;

      const scene = new ThreeScene();
      const camera = createCamera();
      const hemisphere = new HemisphereLight(undefined, undefined, 1.6);
      const sun = new DirectionalLight(undefined, 2.2);
      sun.position.set(1.5, 3, 2.5);
      scene.add(hemisphere, sun);
      const floor = new Mesh(
        new CircleGeometry(FLOOR_RADIUS, 64),
        new MeshStandardMaterial({ roughness: 1 }),
      );
      floor.rotation.x = -Math.PI / 2;
      scene.add(floor);
      const alpha = blobAlpha();
      const blob = (): Mesh<CircleGeometry, MeshBasicMaterial> => {
        const mesh = new Mesh(
          new CircleGeometry(BLOB_RADIUS, 32),
          new MeshBasicMaterial({ alphaMap: alpha, transparent: true, depthWrite: false }),
        );
        mesh.rotation.x = -Math.PI / 2;
        mesh.position.y = BLOB_LIFT;
        scene.add(mesh);
        return mesh;
      };
      const blobs = { a: blob(), b: blob() };
      const particles = new InstancedMesh(
        new OctahedronGeometry(1, 0),
        new MeshBasicMaterial(),
        MAX_PARTICLES,
      );
      particles.instanceMatrix.setUsage(DynamicDrawUsage);
      particles.setColorAt(0, tint.setScalar(1));
      particles.count = 0;
      particles.frustumCulled = false;
      scene.add(particles);

      const overlay = createOverlay(container, setup.theme, setup.labels);
      world = { renderer, scene, camera, floor, hemisphere, particles, blobs, overlay };
      paint(world, setup.theme);

      const canvas = renderer.domElement;
      canvas.style.display = 'block';
      canvas.style.width = '100%';
      canvas.style.height = '100%';
      container.insertBefore(canvas, overlay.element);
      resize();
      observer = new ResizeObserver(resize);
      observer.observe(container);
      let last: number | null = null;
      let smoothed = 0;
      let sinceStep = 0;
      renderer.setAnimationLoop((time: number) => {
        const w = world;
        if (w === null) return;
        const elapsed = last === null ? 0 : time - last;
        last = time;
        if (elapsed > 0) setup.onFrame(elapsed);
        if (!dirty) return;
        // A canvas the page does not show: WebKit keeps what is drawn in it and pays it all on the
        // first frame that shows (seconds, with the stage hidden through pre_live). It stays dirty.
        if (container.checkVisibility?.({ visibilityProperty: true }) === false) return;
        dirty = false;
        if (elapsed > 0 && elapsed < IGNORED_GAP_MS) {
          smoothed += (elapsed - smoothed) * FRAME_SMOOTHING;
          sinceStep += elapsed;
          if (
            smoothed > SLOW_FRAME_MS &&
            sinceStep > RATIO_COOLDOWN_MS &&
            pixelRatio > MIN_PIXEL_RATIO
          ) {
            pixelRatio = Math.max(MIN_PIXEL_RATIO, pixelRatio * RATIO_STEP);
            w.renderer.setPixelRatio(pixelRatio);
            sinceStep = 0;
            smoothed = 0;
            resize();
          }
        }
        w.renderer.render(w.scene, w.camera);
      });
    },
    async loadAssets() {
      const next = botarga ?? (await loadBotarga(model));
      if (destroyed) return;
      botarga = next;
      disposeFighters();
      const created = {
        a: createFighter(next, 'a', model.tints?.a, lite),
        b: createFighter(next, 'b', model.tints?.b, lite),
      };
      for (const side of SIDES) world?.scene.add(created[side].root);
      fighters = created;
      // Compiled and uploaded now, while the stage may still be hidden: on the first visible frame
      // they would block the page (seconds on a weak GPU). Reading a pixel makes the browser do
      // that work here instead of on the first frame it shows.
      const w = world;
      if (w !== null) {
        await w.renderer.compileAsync(w.scene, w.camera);
        if (destroyed) return;
        w.renderer.render(w.scene, w.camera);
        const gl = w.renderer.getContext();
        gl.readPixels(0, 0, 1, 1, gl.RGBA, gl.UNSIGNED_BYTE, new Uint8Array(4));
      }
      if (destroyed) return;
      lastScene = null;
      dirty = true;
    },
    apply(scene) {
      lastScene = scene;
      dirty = true;
      if (world !== null) draw(world, scene);
    },
    setTheme(next) {
      theme = next;
      const w = world;
      if (w === null) return;
      paint(w, next);
      w.overlay.setTheme(next);
      if (lastScene !== null) draw(w, lastScene);
      dirty = true;
    },
    destroy() {
      if (destroyed) return;
      destroyed = true;
      observer?.disconnect();
      disposeFighters();
      const w = world;
      world = null;
      if (w !== null) {
        w.renderer.setAnimationLoop(null);
        w.floor.geometry.dispose();
        w.floor.material.dispose();
        w.particles.geometry.dispose();
        w.particles.material.dispose();
        w.particles.dispose();
        for (const side of SIDES) {
          w.blobs[side].geometry.dispose();
          w.blobs[side].material.dispose();
        }
        w.blobs.a.material.alphaMap?.dispose();
        w.overlay.destroy();
        w.renderer.domElement.remove();
        w.renderer.dispose();
        // As Pixi does: without it the context lives until the GC, and a page that watches fight
        // after fight piles them up until the browser drops the oldest (or, WebKit, crashes).
        w.renderer.forceContextLoss();
      }
      if (botarga !== null) {
        for (const texture of botarga.faces.values()) texture.dispose();
        botarga.gltf.scene.traverse((node) => {
          const mesh = node as Mesh;
          if (!mesh.isMesh) return;
          mesh.geometry.dispose();
          const material = mesh.material as MeshStandardMaterial;
          for (const map of [material.map, material.normalMap, material.roughnessMap])
            map?.dispose();
          material.dispose();
        });
        botarga = null;
      }
    },
  };
}
