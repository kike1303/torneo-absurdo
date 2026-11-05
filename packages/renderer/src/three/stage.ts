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
const SHADOW_MAP = 1024;
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
  readonly overlay: Overlay;
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
  let formatNumber: (n: number) => string = String;
  const numbers: DamageNumberEffect[] = [];
  const matrix = new Matrix4();
  const tint = new Color();

  const paint = (w: World, current: RendererTheme): void => {
    const bg = new Color(current.colors.bg);
    w.scene.background = bg;
    w.floor.material.color.copy(bg).lerp(new Color(current.colors.text), FLOOR_MIX);
    w.hemisphere.groundColor.copy(bg);
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
    w.renderer.render(w.scene, w.camera);
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
      let renderer: WebGLRenderer;
      try {
        renderer = new WebGLRenderer({ antialias: true, powerPreference: 'high-performance' });
      } catch (cause) {
        throw new RendererError('webgl_unavailable', { cause });
      }
      if (destroyed) {
        renderer.dispose();
        return;
      }
      renderer.setPixelRatio(Math.min(window.devicePixelRatio || 1, MAX_PIXEL_RATIO));
      renderer.outputColorSpace = SRGBColorSpace;
      renderer.toneMapping = NeutralToneMapping;
      renderer.shadowMap.enabled = true;

      const scene = new ThreeScene();
      const camera = createCamera();
      const hemisphere = new HemisphereLight(undefined, undefined, 1.6);
      const sun = new DirectionalLight(undefined, 2.2);
      sun.position.set(1.5, 3, 2.5);
      sun.castShadow = true;
      sun.shadow.mapSize.set(SHADOW_MAP, SHADOW_MAP);
      Object.assign(sun.shadow.camera, { left: -2, right: 2, top: 2, bottom: -1 });
      scene.add(hemisphere, sun);
      const floor = new Mesh(
        new CircleGeometry(FLOOR_RADIUS, 64),
        new MeshStandardMaterial({ roughness: 1 }),
      );
      floor.rotation.x = -Math.PI / 2;
      floor.receiveShadow = true;
      scene.add(floor);
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
      world = { renderer, scene, camera, floor, hemisphere, particles, overlay };
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
      renderer.setAnimationLoop((time: number) => {
        const w = world;
        if (w === null) return;
        const elapsed = last === null ? 0 : time - last;
        last = time;
        if (elapsed > 0) setup.onFrame(elapsed);
        w.renderer.render(w.scene, w.camera);
      });
    },
    async loadAssets() {
      const next = botarga ?? (await loadBotarga(model));
      if (destroyed) return;
      botarga = next;
      disposeFighters();
      const created = {
        a: createFighter(next, 'a', model.tints?.a),
        b: createFighter(next, 'b', model.tints?.b),
      };
      for (const side of SIDES) world?.scene.add(created[side].root);
      fighters = created;
      lastScene = null;
    },
    apply(scene) {
      lastScene = scene;
      if (world !== null) draw(world, scene);
    },
    setTheme(next) {
      theme = next;
      const w = world;
      if (w === null) return;
      paint(w, next);
      w.overlay.setTheme(next);
      if (lastScene !== null) draw(w, lastScene);
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
        w.overlay.destroy();
        w.renderer.domElement.remove();
        w.renderer.dispose();
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
