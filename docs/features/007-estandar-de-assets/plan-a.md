---
id: "007"
title: "Estándar de assets, fase A: contrato en el renderer y atadura al ruleset"
type: plan
status: done
risk: medium
author: frontend-engineer
reviewers: [architect]
approvals:
  - by: architect
    date: 2026-10-28
linear: TA-22
depends_on: ["002", "003", "006"]
created: 2026-10-28
updated: 2026-10-28
---

# Plan: Estándar de assets, fase A (desde spec.md 2026-10-27)

> [intent](./intent.md) · [spec](./spec.md) · [plan-a1 de 002](../002-spike-renderer/plan-a1.md) · [plan de 005](../005-timeline-v2-fines-sin-ko/plan.md) · ADRs [0001](../../decisions/0001-motor-propio-determinista.md), [0002](../../decisions/0002-commit-reveal-semilla.md), [0007](../../decisions/0007-i18n-desde-cero.md)

Alcance: RF-A1 a RF-A6 de la spec. No toca `crates/engine`, `crates/engine-wasm`, `crates/protocol`, `packages/protocol-ts`, `apps/server`, `apps/web`, `tools/`, `packages/renderer/placeholders/**`, el formato `torneo.sprites` v1 ni el `justfile`. El export, el importador, la procedencia, el vínculo, la guía de `docs/` y el chequeo de sets son de la fase B (`plan-b.md`).

## Para el architect: lo que tienes que decidir
1. **Código `guide.too_many_errors`.** RF-A3 pide reutilizar `ErrorSink` y `LIMITS.maxErrors`. `ErrorSink` necesita un código para el corte, y la tabla de reglas de la spec no lo lista. Propongo `guide.too_many_errors` (el mismo patrón que `manifest.too_many_errors`) en `guide/codes.ts`, con su test. No es una regla de la guía, así que la tabla "Guía: reglas validadas" no cambia.
2. **Entrada de píxeles inconsistente.** Si `sheets.length !== manifest.sheets.length`, si un `RgbaImage` no mide lo que dice `manifest.sheets[i]` o si `data.length !== width × height × 4`, `validateGuide` lanza `RangeError('guide.sheets_mismatch')` en lugar de devolver un error de la guía. Es un error de quien llama (la fase B comprueba antes el IHDR con `set.sheet_header`), no un defecto del set. El precedente es `resolveAnimation`, que lanza `manifest.missing_animation` en un caso imposible con un manifest validado. La alternativa sería un código `guide.*` más que la spec no tiene.
3. **Cuándo `guide.windup_long` lleva `ruleset_version`.** Solo cuando el ruleset es el límite **estricto**: si `min_v windup_<move>_ms(v)` es menor que el tope estructural (`LIMITS.windupMaxMs` y, en `attack_light`, también el máximo de 250 de la guía). Ante un empate manda el tope estructural y `data` no lleva versión. Entre versiones empatadas en el mínimo se reporta la más baja. Así, en v1 (400/700/1100), ningún error lleva versión.

Si las tres te parecen bien, quedan como decisiones 9, 10 y 11 sin más cambios.

## Decisiones de implementación
1. **Tope de windup derivado (RF-A1).** `manifest/types.ts` importa `TIMELINE_LIMITS` de `../timeline/types.ts`, que no importa nada (no hay ciclo: `timeline/validate.ts` importa `manifest/types.ts`, pero `types.ts` no importa `validate.ts`). Exporta `REACTION_WINDOW_MS = 200` con un comentario que cita la garantía de la coreografía de 002, y dentro del `Object.freeze` de `LIMITS` queda `windupMaxMs: TIMELINE_LIMITS.strikeGapMs - REACTION_WINDOW_MS`. `scene/choreography.ts` pasa de `import type { AnimationCode }` a importar también el **valor** `LIMITS`, y queda `export const MAX_WINDUP_MS = LIMITS.windupMaxMs;`, con el comentario actualizado ("derived from the manifest limit, 007 RF-A1"). `LOOKAHEAD_MS = MAX_WINDUP_MS` en `scene/prepare.ts` no cambia. `validate.ts` del manifest no se toca: sigue leyendo `LIMITS.windupMaxMs`, que vale 400.
2. **Módulo `src/guide/`, puro.** Seis archivos, los de la spec:
   - `limits.ts`: `GUIDE_LIMITS`, congelado en profundidad (cada objeto anidado con `Object.freeze`), con exactamente los valores de "Guía: números" y de la tabla por acción. Forma:
     ```ts
     GUIDE_LIMITS = {
       cellPx: 192, groundMarginPx: 8, heightMinPx: 64, heightMaxPx: 176, feetTolerancePx: 2,
       frameMsMin: 33, frameMsMax: 1000, keyPoseMinMs: 66, windupStepMs: 100,
       sheetFileMaxBytes: 4 * 1024 * 1024, exportFileMaxBytes: 1024 * 1024,
       animations: {
         idle:         { framesMin: 4, framesMax: 12, distinctMin: 3, frameMaxMs: 250, cycleMinMs: 600, cycleMaxMs: 2400 },
         walk:         { framesMin: 4, framesMax: 12, cycleMinMs: 300, cycleMaxMs: 1200 },
         intro:        { framesMin: 3, framesMax: 24 },
         attack_light: { framesMin: 3, framesMax: 8,  windupMinMs: 100, windupMaxMs: 250, impactFrameMin: 1, impactHoldMinMs: 50 },
         attack_heavy: { framesMin: 4, framesMax: 10, windupMinMs: 250, impactFrameMin: 2, impactHoldMinMs: 66 },
         special:      { framesMin: 5, framesMax: 14, windupMinMs: 300, impactFrameMin: 3, impactHoldMinMs: 66 },
         hit:   { framesMin: 2, framesMax: 6, totalMaxMs: 1000 },
         block: { framesMin: 2, framesMax: 6, totalMaxMs: 1000 },
         dodge: { framesMin: 2, framesMax: 6, totalMaxMs: 1000 },
         knockdown: { framesMin: 3, framesMax: 12 },
         ko:        { framesMin: 3, framesMax: 12 },
         win:       { framesMin: 2, framesMax: 12 },
       },
     }
     ```
     Los derivados **no** son campos: pivote `(cellPx / 2, cellPx − groundMarginPx)`, máximo de `intro` (`introGapMs − windupMaxMs`), mínimo de reacciones (`REACTION_WINDOW_MS`), `frame_ms[0]` máximo de reacciones (`REACTION_WINDOW_MS − keyPoseMinMs`), máximo de `knockdown` (`min_v knockdown_ms`), máximo de `ko` (`winGapMs`), recuperación mínima (`REACTION_WINDOW_MS`), alcance (`CONTACT_PX / 2`) y tope de windup (`windupCap`). `sheetFileMaxBytes` y `exportFileMaxBytes` entran ya porque RF-A2 pide "los valores de la tabla y nada más", aunque solo los usa la fase B.
   - `codes.ts`: `GUIDE_CODES`, congelado, con los 22 códigos de la tabla de la spec más `guide.too_many_errors` (punto 1 de arriba), y `RULESET_CODES = { invalid: 'ruleset.invalid' }`.
   - `ruleset.ts`: tipo `RulesetTiming` (`ruleset_version`, `windup_strike_ms`, `windup_heavy_ms`, `windup_special_ms`, `knockdown_ms`, en snake_case como el dorado) y `parseRulesetTiming(json: unknown): { ok: true; rulesets: readonly RulesetTiming[] } | { ok: false; errors: readonly RulesetError[] }`, con `RulesetError = { code: 'ruleset.invalid'; path: string }`. Usa los helpers de `manifest/input.ts` (`isPlainObject`, `own`, `pointer`, `isInt`) y un `ErrorSink` con el mismo tope. Reglas: raíz objeto con la llave exacta `versions`; `versions` es un array no vacío; cada entrada es un objeto con las cinco llaves exactas; cada valor es un entero seguro ≥ 0; `ruleset_version` estrictamente creciente (eso cubre "únicas y ordenadas"). El `path` apunta al lugar exacto (`/versions/1/ruleset_version`, `/versions/0/extra`, `/versions/0/knockdown_ms`). La fase B suma `strike_gap_min_ms` a las llaves exactas cuando 006 esté en main.
   - `table.ts`: `windupCap(move: AttackAnimation, rulesets): number` = `min(LIMITS.windupMaxMs, min_v windup_<move>_ms(v))`, con el mapa `attack_light → windup_strike_ms`, `attack_heavy → windup_heavy_ms`, `special → windup_special_ms` (lanza `RangeError('guide.no_rulesets')` con `rulesets` vacío, para que nunca devuelva un tope sin versiones). Un helper interno `windupBound(move, rulesets) → { max, ruleset_version? }` aplica la decisión 11. `guideTable(rulesets): readonly GuideRow[]`, una fila por animación en orden canónico (`REQUIRED_ANIMATIONS` y luego `OPTIONAL_ANIMATIONS`), con los rangos efectivos ya resueltos:
     ```ts
     interface GuideRange { readonly min: number; readonly max: number }
     interface GuideRow {
       readonly animation: AnimationCode;
       readonly required: boolean;
       readonly frames: GuideRange;
       readonly distinct_min?: number;          // idle
       readonly frame_max_ms?: number;          // idle
       readonly cycle_ms?: GuideRange;          // idle, walk
       readonly duration_ms?: GuideRange;       // intro, hit, block, dodge, knockdown, ko (min 0 si solo hay tope)
       readonly windup_ms?: GuideRange & { readonly ruleset_version?: number }; // ataques
       readonly impact_frame_min?: number;      // ataques
       readonly impact_hold_min_ms?: number;    // ataques
       readonly recovery_min_ms?: number;       // ataques: REACTION_WINDOW_MS
       readonly first_frame_max_ms?: number;    // hit, block, dodge: 134
     }
     ```
     Solo códigos y números, sin texto (RF-A6). Con `rulesets` vacío lanza `RangeError('guide.no_rulesets')`.
   - `validate.ts`: `validateGuide(manifest: SpriteManifest, sheets: readonly RgbaImage[], rulesets: readonly RulesetTiming[]): GuideResult`, con `RgbaImage = { readonly width: number; readonly height: number; readonly data: Uint8Array }`, `GuideError = { code: \`guide.${string}\`; path: string; data?: Readonly<Record<string, string | number>> }` y `GuideResult = { ok: true } | { ok: false; errors: readonly GuideError[] }`. **Lee los rangos de `guideTable(rulesets)`**, así la tabla de la fase B y la validación nunca divergen.
   - `index.ts`: reexporta `GUIDE_LIMITS`, `GUIDE_CODES`, `RULESET_CODES`, `parseRulesetTiming`, `windupCap`, `guideTable`, `validateGuide` y los tipos.
3. **Orden y forma de los errores de `validateGuide`.** Determinista, en una pasada y con un solo `ErrorSink` (`LIMITS.maxErrors`, `guide.too_many_errors`):
   1. `rulesets` vacío → devuelve solo `[{ code: 'guide.no_rulesets', path: '' }]` y corta.
   2. Precondición de píxeles (decisión 10).
   3. `guide.scale_forbidden` en `/scale`.
   4. Por frame `i` de `manifest.frames`, cada índice una sola vez: `cell_size` (`/frames/i`, `{ w, h }`); si mide la celda, `pivot_position` (`/frames/i`, `{ pivot_x, pivot_y }`), `frame_empty` (`/frames/i`) y, si no está vacío, `frame_clipped` (`/frames/i`, borde exterior de la celda: fila 0, fila `cellPx − 1`, columna 0 y columna `cellPx − 1`). "Opaco" es alfa > 0. Para cada frame que mide la celda se calcula una vez su caja opaca (fila más alta, fila más baja, columna más a la derecha) y se guarda por índice; las reglas por animación la reutilizan (costo lineal en píxeles, "cada frame distinto una sola vez").
   5. Por animación presente, en orden canónico: `frames_count` (`/animations/<a>/frames`, `{ count, min, max }`); `frame_ms_range` por paso (`/animations/<a>/frame_ms/<k>`, `{ ms }`); después las propias de la fila: `idle_distinct` (`/animations/idle/frames`, `{ count }`, contando contenido de píxeles distinto, comparando los bytes de la celda, no índices, para que dos índices con la misma imagen no cuenten doble), `idle_frame_long` (`/animations/idle/frame_ms/<k>`, `{ ms }`), `cycle_range` (`/animations/<a>/frame_ms`, `{ total_ms, min, max }`), `duration_range` (`/animations/<a>/frame_ms`, `{ total_ms, min, max }`), `windup_short` (`/animations/<a>/impact_frame`, `{ windup_ms, min }`), `windup_long` (`/animations/<a>/impact_frame`, `{ windup_ms, cap_ms }` más `ruleset_version` según la decisión 11), `anticipation` (`/animations/<a>/impact_frame`, `{ impact_frame, min }`), `impact_hold` (`/animations/<a>/frame_ms/<impact>`, `{ ms, min }`), `recovery_short` (`/animations/<a>/frame_ms`, `{ recovery_ms, min }`), `reaction_slow` (`/animations/<a>/frame_ms/0`, `{ ms, max }`), y las de píxeles: `feet_off_pivot` (una vez por frame distinto de `idle`, en el primer paso que lo usa, `/animations/idle/frames/<k>`, `{ row }`), `height_range` (`idle` paso 0, `/animations/idle/frames/0`, `{ height }`) e `impact_short` (paso `impact_frame` de cada ataque, `/animations/<a>/frames/<impact>`, `{ reach }`, con `reach = columna opaca más a la derecha − pivot_x`). Las reglas de píxeles saltan un frame que no mide la celda o que está vacío (ya tiene su error; sin cascada). W y R se calculan con `windupMs` de `manifest/resolve.ts` y con la suma desde `impact_frame` hasta el final.
   6. `guide.windup_order`, al final y solo si están los tres ataques (siempre, en un manifest validado): un error en `/animations/attack_heavy/impact_frame` si `W(light) + windupStepMs > W(heavy)`, y otro en `/animations/special/impact_frame` si `W(heavy) > W(special)`. Los dos con `data` `{ light_ms, heavy_ms, special_ms, min_step }`.
4. **Subpath y frontera de runtime (RF-A2).** `packages/renderer/package.json` suma `"./guide": { "types": "./src/guide/index.ts", "default": "./src/guide/index.ts" }`. `src/index.ts` **no** reexporta `guide/`. `guide/` puede importar de `manifest/` (tipos, `LIMITS`, `REQUIRED_ANIMATIONS`, `windupMs`, `ErrorSink`), de `timeline/types.ts` (`TIMELINE_LIMITS`) y de `scene/choreography.ts` (`CONTACT_PX`), que son puros. `eslint.config.js` suma `'src/guide/**/*.ts'` a `PURE` (sin Pixi ni DOM), con un caso nuevo en `test/eslint.test.ts`. La comprobación de que el runtime no lo carga es un **test de imports** (`test/guide-imports.test.ts`), no `check-renderer-split`: ese script corre solo en `just e2e` y mira el build de la web, que nunca importa `guide/`. El test recorre los imports estáticos relativos (`import … from './x.ts'`, `export … from`, `import './x.ts'`) desde `src/index.ts` y desde `src/stage/index.ts`, resuelve las rutas y comprueba que ninguna cae en `src/guide/`. Recorre también el cierre de `src/guide/index.ts` y comprueba que no llega a `src/stage/`, `src/player/` ni a `pixi.js`. Por último, comprueba que `package.json` declara el subpath `./guide` con esas rutas.
5. **Dorado del ruleset (RF-A5), en Rust.** Test nuevo `crates/content/tests/ruleset_timing.rs`, solo con la API pública del motor (`engine::Ruleset::for_version` y los accessors `version()`, `windup_strike_ms()`, `windup_heavy_ms()`, `windup_special_ms()` y `knockdown_ms()`):
   ```rust
   //! 007 RF-A5: the ruleset timings the sprite guide reads (packages/renderer/src/guide).
   #[derive(Serialize)]
   struct Timing { knockdown_ms: u16, ruleset_version: u16, windup_heavy_ms: u32,
                   windup_special_ms: u32, windup_strike_ms: u32 }   // fields in alphabetical order
   #[derive(Serialize)]
   struct Golden { versions: Vec<Timing> }
   fn golden() -> Golden {
       Golden { versions: (0..=u16::MAX).filter_map(Ruleset::for_version).map(|r| Timing { … }).collect() }
   }
   ```
   - **Bytes canónicos:** `serde_json::to_vec_pretty` (2 espacios) más `\n` final. Las llaves salen ordenadas porque los campos del `struct` se declaran en orden alfabético; no depende de que `serde_json` tenga o no `preserve_order`. Un segundo test, `timing_fields_are_sorted`, serializa un `Timing` a `serde_json::Value` y comprueba que las llaves en el orden de los bytes están ordenadas, para que nadie agregue un campo fuera de orden.
   - **Comparación y regeneración:** igual que `check_golden` de `crates/engine/tests/golden.rs`. Con `UPDATE_GOLDEN=1` (y `CI` ausente, con un `assert!` que lo exige) escribe `crates/content/tests/golden/ruleset-timing.json` y termina. Si no, lee el archivo y, si difiere, hace `panic!` con este mensaje (en inglés, es código): `"ruleset-timing.json: bytes differ (first difference at {at:?}); a ruleset version changed or was added: regenerate with UPDATE_GOLDEN=1 cargo test -p content --test ruleset_timing and run just check again, which revalidates every sprite set (007 RF-A5)"`. Ese es el fallo explícito del criterio de éxito.
   - Un tercer test, `every_listed_version_exists`, comprueba que el dorado no está vacío y que `Ruleset::for_version(1)` está en él.
   - `crates/content` no tiene `clippy.toml` propio, así que `std::fs::write` y `std::env::var_os` están permitidos (el de `crates/engine` los prohíbe, pero no aplica). No hay dependencias nuevas: `serde` (con `derive`) y `serde_json` ya son dependencias normales del crate, y los tests de integración las ven. `_check-wasm` solo corre `-p engine`, así que este test no corre en WASI.
   - `.prettierignore` suma `crates/content/tests/golden/` con un comentario ("Ruleset timing golden: exact canonical bytes plus a final newline (007 RF-A5)"), junto a la línea de `crates/engine/tests/golden/`.
6. **Vitest de atadura del dorado.** `test/guide-ruleset.test.ts` lee el dorado real con `readFileSync(new URL('../../../crates/content/tests/golden/ruleset-timing.json', import.meta.url))` (no hay import de JSON entre paquetes, así que la frontera de ESLint no cambia) y comprueba que `parseRulesetTiming` lo acepta. Además, para **cada** versión del dorado, que la guía se puede cumplir: `windupCap(m) ≥ windupMinMs(m)` para los tres ataques, `windupCap('attack_heavy') ≥ windupMinMs('attack_light') + windupStepMs` (el orden es satisfacible) y `knockdown_ms ≥ framesMin(knockdown) × frameMsMin`. Si una versión nueva baja un `windup_*` hasta dejar la guía imposible, falla aquí con la versión en el mensaje, además del dorado de Rust.
7. **Referencia de prueba sin PNG.** `test/support/guide-ref.ts` arma en memoria un manifest válido para `validateManifest` y para la guía, más su hoja como `RgbaImage` (sin `pngjs`): una hoja de `4 × 192` por `192` con cuatro celdas. F0, F1 y F2 son el cuerpo en `idle` (un rectángulo opaco de 40 px de ancho, de la fila 64 a la 183, es decir, alto 120 y pies en `pivotY − 1`, con una diferencia de un píxel entre F0, F1 y F2). F3 es el cuerpo con un brazo hasta la columna 136 (`reach` 40 ≥ 32). Pivote (96, 184) en los cuatro frames, sin píxeles en el borde. Tiempos (los de `dummy_a`, que la spec y game-designer comprobaron que cumplen la guía):

   | Animación | `frames` | `frame_ms` | Comprobación |
   |---|---|---|---|
   | `idle` (loop) | 0,1,2,1 | 200,250,200,250 | ciclo 900, 3 distintos, ≤ 250 |
   | `walk` (loop) | 0,1,0,1 | 110 × 4 | ciclo 440 |
   | `intro` | 0,1,2 | 200 × 3 | 600 ≤ 1600 |
   | `attack_light` | 0,0,3,0 | 60,60,90,120, impacto 2 | W 120, mantiene 90, R 210 |
   | `attack_heavy` | 0,0,0,3,0 | 100,100,100,150,220, impacto 3 | W 300 ≥ 120 + 100, R 370 |
   | `special` | 0,0,0,0,3,0 | 80,80,80,80,160,260, impacto 4 | W 320 ≥ 300, R 420 |
   | `hit` | 0,1,0 | 80,120,200 | total 400, primero 80 ≤ 134 |
   | `block` | 0,1 | 100,250 | total 350 |
   | `dodge` | 0,1,0 | 80,160,160 | total 400 |
   | `knockdown` | 0,1,2,0 | 100,150,250,400 | 900 ≤ 1500 |
   | `ko` (sin loop) | 0,1,2,0 | 100,150,250,500 | 1000 ≤ 1500 |
   | `win` | 0,1,0,1 | 200 × 4 | 4 frames |

   Expone `refManifest()` (JSON sin validar, para aplicar `applyOps` de `support/patch.ts`), `refSheets()` (copia nueva de los píxeles cada vez), `paint(sheets, frame, fn)` para mutar una celda y `V1` (el dorado real ya parseado). Cada caso de fallo parte de la referencia, cambia una sola cosa y vuelve a pasar por `validateManifest` antes de `validateGuide`.
8. **Placeholders fuera de la capa de guía.** No se tocan ni se regeneran. `test/placeholders.test.ts` sigue igual (pasan `validateManifest`). Se suma un caso a `test/guide-validate.test.ts` que documenta por qué quedan fuera: `validateGuide(DUMMY_A, …)` y `validateGuide(DUMMY_B, …)` (con sus hojas decodificadas con `pngjs`, como ya hace `placeholders.test.ts`) fallan con `guide.cell_size` en todos sus frames (64×96 y 80×88), y `dummy_b` además con `guide.windup_order` (especial 320 < pesado 380). El test de imports (decisión 4) prueba que ningún camino del runtime ejecuta la guía.
9. **`guide.too_many_errors`:** ver "Para el architect", punto 1.
10. **Precondición de píxeles:** ver "Para el architect", punto 2.
11. **Versión en `guide.windup_long`:** ver "Para el architect", punto 3.

## Coordinación con 006 (TA-21, PR #46, rama `ta-21-faster-pacing`)
Revisado con `git diff origin/main...origin/ta-21-faster-pacing` (52 archivos):

| Archivo de la fase A | ¿Lo toca 006? |
|---|---|
| `packages/renderer/src/manifest/types.ts`, `scene/choreography.ts`, `src/guide/**`, `package.json`, `eslint.config.js` | No (006 no toca `packages/renderer/src` ni la configuración del paquete) |
| `packages/renderer/test/*.test.ts` nuevos, `test/support/guide-ref.ts`, `test/eslint.test.ts` | No |
| `crates/content/tests/ruleset_timing.rs`, `crates/content/tests/golden/ruleset-timing.json` | No (006 no toca `crates/content`) |
| `.prettierignore` | No |

**No hay archivos en común.** Lo que comparten es semántico:
- `crates/engine/src/ruleset.rs`: 006 agrega `breather_*`, `strike_gap_min()` y `breather_gap_min()`, pero **no** cambia `windup_*_ms` ni `knockdown_ms` (su spec aprobada lo dice y el diff lo confirma). La fase A no edita `ruleset.rs` ni ningún archivo de `crates/engine`: solo llama a `Ruleset::for_version` y a cinco accessors que ya existen en main. Tampoco usa `strike_gap_min()` ni `TIMELINE_CONTRACT`: son de la fase B.
- `packages/renderer/test/fixtures/golden/*.timeline.json`: 006 las regenera con el ritmo nuevo. La prueba de propiedad de RF-A1 corre también sobre `goldenTimeline(...)`, así que tiene que pasar con las dos versiones.
- `apps/web/src/lib/features/arena/retime.ts` (`?pace`): 006 lleva separaciones cerca de 600 ms. La propiedad de RF-A1 genera timelines con separaciones exactas de 600 ms y windups de 400, que es el peor caso que `?pace` puede producir.

**Orden de merge: indiferente para la fase A.** Sale cuando se apruebe, sin esperar a 006.
- **Si 006 se fusiona primero:** `git rebase origin/main` (no espero conflictos). Después corro `cargo test -p content --test ruleset_timing` **sin** `UPDATE_GOLDEN`: tiene que pasar sin regenerar, porque 006 no cambia tiempos de windup ni de knockdown. Si fallara, no se regenera a ciegas: significa que 006 cambió un tiempo contra su spec, y lo llevo al architect. También corro `pnpm --filter @torneo/renderer exec vitest run` para que la propiedad de RF-A1 corra contra las timelines doradas nuevas, y por último `just check` completo antes de hacer push.
- **Si la fase A se fusiona primero:** 006 no necesita nada; su rebase no toca archivos de la fase A. Dejo una nota en el PR #46 avisando que existe `crates/content/tests/golden/ruleset-timing.json` y que no cambia con 006.
- **Fase B:** sí depende de 006 (`strike_gap_min_ms` en el dorado y la fila de recuperación recomendada) y comparte con 006 `docs/architecture.md` y la spec de 002. Eso lo resuelve `plan-b.md`.

## Archivos que cambian
| Archivo | Cambio |
|---|---|
| `packages/renderer/src/manifest/types.ts` | modificado: importa `TIMELINE_LIMITS`; nuevo `REACTION_WINDOW_MS = 200`; `LIMITS.windupMaxMs = TIMELINE_LIMITS.strikeGapMs − REACTION_WINDOW_MS` (RF-A1) |
| `packages/renderer/src/scene/choreography.ts` | modificado: `MAX_WINDUP_MS = LIMITS.windupMaxMs` (sin literal) y comentario (RF-A1) |
| `packages/renderer/src/guide/limits.ts` | nuevo: `GUIDE_LIMITS` congelado en profundidad (RF-A2) |
| `packages/renderer/src/guide/codes.ts` | nuevo: `GUIDE_CODES` (22 de la spec + `too_many_errors`), `RULESET_CODES` (RF-A3, RF-A5) |
| `packages/renderer/src/guide/ruleset.ts` | nuevo: `RulesetTiming`, `parseRulesetTiming` (RF-A5) |
| `packages/renderer/src/guide/table.ts` | nuevo: `windupCap`, `windupBound` (interno), `guideTable`, `GuideRow` (RF-A4, RF-A6) |
| `packages/renderer/src/guide/validate.ts` | nuevo: `validateGuide`, `RgbaImage`, `GuideError`, `GuideResult` (RF-A3, RF-A4) |
| `packages/renderer/src/guide/index.ts` | nuevo: API del subpath (RF-A2) |
| `packages/renderer/package.json` | modificado: subpath `./guide` (RF-A2) |
| `packages/renderer/eslint.config.js` | modificado: `src/guide/**/*.ts` en `PURE` (RF-A2) |
| `packages/renderer/test/eslint.test.ts` | modificado: `pixi.js` y `document` prohibidos en `src/guide/x.ts` |
| `packages/renderer/test/windup-derivation.test.ts` | nuevo: RF-A1 unitario |
| `packages/renderer/test/reaction-window.test.ts` | nuevo: propiedad de RF-A1 sobre `sceneAt` |
| `packages/renderer/test/guide-limits.test.ts` | nuevo: RF-A2 |
| `packages/renderer/test/guide-ruleset.test.ts` | nuevo: RF-A5 (TS) y atadura (decisión 6) |
| `packages/renderer/test/guide-validate.test.ts` | nuevo: RF-A3, RF-A4 (cada código, `windup_order`, placeholders) |
| `packages/renderer/test/guide-table.test.ts` | nuevo: RF-A6 |
| `packages/renderer/test/guide-imports.test.ts` | nuevo: frontera de runtime (RF-A2) |
| `packages/renderer/test/support/guide-ref.ts` | nuevo: manifest y píxeles de referencia (decisión 7) |
| `crates/content/tests/ruleset_timing.rs` | nuevo: dorado (RF-A5) |
| `crates/content/tests/golden/ruleset-timing.json` | nuevo: generado con `UPDATE_GOLDEN=1`, commiteado |
| `.prettierignore` | modificado: `crates/content/tests/golden/` |
| `docs/features/007-estandar-de-assets/plan-a.md` | este plan; "Desviaciones" si hace falta |

No cambian: `packages/renderer/src/manifest/validate.ts`, `src/index.ts`, `src/scene/prepare.ts`, los placeholders, `justfile` (los `tsc`, `eslint` y `vitest` del renderer y `cargo test --workspace` ya cubren todo lo nuevo), los workflows y `Cargo.lock`/`pnpm-lock.yaml` (sin dependencias nuevas).

## Orden de trabajo
1. **Dorado de Rust** (decisión 5): `ruleset_timing.rs`, `UPDATE_GOLDEN=1 cargo test -p content --test ruleset_timing`, revisar a mano que el JSON tiene una versión con 400/700/1100/1500, `.prettierignore`. Commit `test(content): add ruleset timing golden (TA-22)`. **Paralelizable** con los pasos 2 y 3 (archivos distintos, sin dependencias).
2. **RF-A1** (decisión 1): primero `windup-derivation.test.ts` y `reaction-window.test.ts` contra el código actual (tienen que pasar ya, porque los valores no cambian: es la foto de antes), después el cambio en `types.ts` y `choreography.ts`, y los dos tests siguen en verde. Commit `refactor(renderer): derive the windup cap from the timeline rules (TA-22)`.
3. **`guide/limits.ts` y `guide/codes.ts`** con `guide-limits.test.ts`. **Paralelizable** con 1 y 2.
4. **`guide/ruleset.ts`** con `guide-ruleset.test.ts` (necesita el dorado del paso 1).
5. **`guide/table.ts`** (`windupCap`, `guideTable`) con `guide-table.test.ts` (necesita 3 y 4).
6. **`test/support/guide-ref.ts`** y **`guide/validate.ts`** con `guide-validate.test.ts`, regla por regla: primero el caso válido de referencia, después cada código con su caso (rojo → verde). Es el paso más largo.
7. **Frontera:** `guide/index.ts`, subpath en `package.json`, `PURE` en ESLint, `eslint.test.ts` y `guide-imports.test.ts`. Commit `feat(renderer): add the sprite guide layer (TA-22)` (pasos 3 a 7, o uno por paso si queda grande).
8. `just fmt`, `just check` completo, salida pegada en el PR. Si 006 ya está en main, antes el rebase de "Coordinación con 006".

## Riesgos
- **Ciclo de imports o un orden de evaluación nuevo** (`choreography.ts` pasa a importar un valor de `manifest/types.ts`, y `manifest/types.ts` uno de `timeline/types.ts`). → `timeline/types.ts` no importa nada, así que el grafo sigue siendo acíclico; `tsc` y los tests de escena lo detectan si no. `windup-derivation.test.ts` comprueba que `MAX_WINDUP_MS` no es `undefined` (que es como se manifiesta un ciclo en ESM) y vale 400.
- **Cambiar el comportamiento del renderer sin querer.** → El valor sigue en 400; el fixture `windup_too_long.json` (401 ms) sigue fallando igual; `scene.test.ts` (`LOOKAHEAD_MS`, la ventana de `cueWindow`) y `choreography.test.ts` no cambian, y la propiedad de RF-A1 se escribe y pasa **antes** del refactor.
- **Bytes del dorado inestables** (orden de llaves, formato). → `struct` con campos en orden alfabético y el test que lo vigila, `to_vec_pretty` y `\n` explícitos, y `.prettierignore` para que Prettier no lo reformatee. Si alguien activa `preserve_order` en `serde_json`, el orden no cambia porque sale del `struct`.
- **Regenerar el dorado en CI por error.** → El mismo `assert!(CI ausente)` que el dorado del motor.
- **La guía y la tabla divergen.** → `validateGuide` lee sus rangos de `guideTable`, y `guide-table.test.ts` comprueba los valores efectivos que cita la spec (1600, 134, 1500, 400/400/400).
- **Una regla de píxeles se dispara en cascada** (un frame vacío también "sin pies", "sin altura"…). → Las reglas de píxeles saltan los frames que ya fallaron `cell_size` o `frame_empty` (decisión 3), y cada caso de test exige la lista **exacta** de errores con `toEqual`.
- **La propiedad de RF-A1 es lenta o inestable.** → PRNG con semilla fija (`mulberry32` del paquete, nunca `Math.random`), 200 timelines de unos 20 impactos y muestras cada 20 ms dentro de la ventana de 200 ms: unas 25 000 llamadas a `sceneAt`, por debajo de 2 s. Si pasa de 3 s, bajo a 100 timelines y lo anoto en "Desviaciones".
- **Un `RangeError` de precondición llega al usuario.** → No hay usuario en la fase A. La fase B comprueba el IHDR (`set.sheet_header`) antes de llamar, y el test cubre el lanzamiento.
- **Coordinación con 006.** → Sin archivos en común; rebase y verificación sin regenerar descritos arriba.

## Alternativas descartadas
- **Usar `check-renderer-split` para probar que el runtime no carga `guide/`.** Corre solo en `just e2e` y sobre el build de la web, que no importa `guide/`, así que pasaría en vacío. El test de imports del paquete corre en `just check` y mira el código fuente del renderer.
- **Dorado como `serde_json::Value` con `json!`.** El orden de llaves dependería de que ninguna crate del workspace active `preserve_order`. El `struct` con campos ordenados es explícito.
- **Dorado compacto en una línea, como `timeline-contract.json` de 006.** La spec pide 2 espacios y salto final; además, un diff de PR con una versión nueva se lee mejor así.
- **Importar el JSON dorado desde el renderer con `import … with { type: 'json' }`.** Ataría el `tsc` del paquete a un archivo de `crates/` y lo metería en el grafo de módulos de `src/`. El dorado solo lo lee un test (y en la fase B, la herramienta).
- **Contar `idle_distinct` por índices de frame.** Un manifest escrito a mano podría repetir la misma imagen con dos índices. Comparar bytes cuesta como mucho 12 celdas.
- **Devolver `guide.no_rulesets` junto con el resto de errores.** Sin rulesets, `windupCap` no tiene sentido; cortar en seco evita errores derivados de un tope inventado.
- **Validar la guía sobre los placeholders.** La spec los deja fuera (otra celda, AGPL, sin migrar); el test solo documenta que fallarían.

## Pruebas (cómo se demuestra que funciona)
- [ ] `just check` en verde, con la salida pegada en el PR (ver "Salida esperada").
- [ ] **RF-A1** (`windup-derivation.test.ts`): `REACTION_WINDOW_MS === 200`; `LIMITS.windupMaxMs === TIMELINE_LIMITS.strikeGapMs − REACTION_WINDOW_MS`; `LIMITS.windupMaxMs === 400` (protección de la fase A: el valor no cambia; si `strikeGapMs` cambia, este caso falla a propósito y obliga a revisar el look-ahead de ADR-0002); `MAX_WINDUP_MS === LIMITS.windupMaxMs`; `LOOKAHEAD_MS === MAX_WINDUP_MS`; y que `choreography.ts` no contiene el literal `= 400` (lectura del fuente con `readFileSync`). El fixture existente `windup_too_long.json` (401) sigue dando `manifest.windup_too_long`.
- [ ] **RF-A1, propiedad** (`reaction-window.test.ts`): para cada timeline y cada impacto a `t` con objetivo X, `sceneAt(prepared, τ).fighters[X].animation` **no** es `attack_light`, `attack_heavy` ni `special` para todo τ en `{t, t+20, …, t+180, t + REACTION_WINDOW_MS − 1}`. Eso prueba a la vez "ningún luchador recibe un impacto durante su propio windup" (en `t`) y "su windup siguiente empieza ≥ `REACTION_WINDOW_MS` después" (en el resto). Además, desde los segmentos de `prepareScene`: todo segmento `windup` de X que empieza después de `t` empieza en `≥ t + REACTION_WINDOW_MS`. Timelines:
  - 200 generadas con `mulberry32` (semillas 1 a 200), construidas con `TimelineBuilder` para que sean válidas por construcción (el test exige `validateTimeline(...).ok` y nunca filtra): actor, movimiento y resultado al azar, separaciones de `{600, 601, 700, 1000, 2500}` ms (600 con más peso) y algún `crit` con knockdown de 800 a 1500 ms (el siguiente impacto, después de que termine);
  - el guion del spike y las tres timelines doradas (`goldenTimeline`), que cambian con 006 y tienen que pasar en los dos estados;
  - tres juegos de manifests: los placeholders, `LONG` y uno nuevo `MAX`, derivado de `dummy_a`/`dummy_b` con los tres windups en exactamente 400 ms (`attack_light` 200,200,…; `attack_heavy` 100,100,200,…; `special` 100,100,100,100,…), validado con `validateManifest`. `MAX` con separaciones de 600 es el borde exacto de C2: el windup empieza justo 200 ms después del impacto.
- [ ] **RF-A2** (`guide-limits.test.ts`): `GUIDE_LIMITS` y cada objeto anidado son `Object.isFrozen`; las llaves de primer nivel y de `animations` son exactamente las de la decisión 2 (y `animations` cubre las 12 animaciones del manifest); coherencia interna: `framesMin ≤ framesMax` en cada fila, `heightMaxPx ≤ cellPx − groundMarginPx − 8`, `cellPx / 2 > CONTACT_PX`, `windupMaxMs(light) + windupStepMs ≤ LIMITS.windupMaxMs` (el orden siempre se puede cumplir), `keyPoseMinMs < REACTION_WINDOW_MS`, `frameMsMin ≥ 33`, `idle.frameMaxMs × idle.framesMax ≥ idle.cycleMinMs`; derivados citados en la spec: pivote (96, 184) e `introGapMs − windupMaxMs = 1600`. `guide-imports.test.ts`: `src/index.ts` y `src/stage/index.ts` no alcanzan `src/guide/`; `src/guide/index.ts` no alcanza `src/stage/`, `src/player/` ni `pixi.js`; `package.json` declara `./guide`. `eslint.test.ts`: `import 'pixi.js'` y `document` en `src/guide/x.ts` dan error.
- [ ] **RF-A3** (`guide-validate.test.ts`): la referencia pasa (`{ ok: true }`) con `[V1]` y con `[V1, V2]` (`V2` sintética con los mismos tiempos). Un caso por código, cada uno con `toEqual({ ok: false, errors: [<el error exacto>] })` (código, `path` y `data`):

  | Código | Cambio sobre la referencia | Error esperado |
  |---|---|---|
  | `no_rulesets` | `rulesets = []` | `''` |
  | `scale_forbidden` | `scale: 1` | `/scale` |
  | `cell_size` | frame 2 con `w: 190` | `/frames/2`, `{ w: 190, h: 192 }` |
  | `pivot_position` | frame 2 con `pivot_x: 95` | `/frames/2`, `{ pivot_x: 95, pivot_y: 184 }` |
  | `frame_ms_range` | `win.frame_ms[1] = 20` | `/animations/win/frame_ms/1`, `{ ms: 20 }` |
  | `frames_count` | `win` con 1 frame (`[0]`, `[200]`) | `/animations/win/frames`, `{ count: 1, min: 2, max: 12 }` |
  | `idle_distinct` | `idle.frames = [0,1,0,1]` | `/animations/idle/frames`, `{ count: 2 }` |
  | `idle_frame_long` | `idle.frame_ms = [300,200,200,200]` | `/animations/idle/frame_ms/0`, `{ ms: 300 }` |
  | `cycle_range` | `walk.frame_ms = [60,60,60,60]` | `/animations/walk/frame_ms`, `{ total_ms: 240, min: 300, max: 1200 }` |
  | `duration_range` | `hit` = `[0,1]`, `[80,80]` | `/animations/hit/frame_ms`, `{ total_ms: 160, min: 200, max: 1000 }` |
  | `windup_short` | `attack_light.frame_ms = [40,40,90,120]` | `/animations/attack_light/impact_frame`, `{ windup_ms: 80, min: 100 }` |
  | `windup_long` (guía) | ligero `[130,130,90,120]` (260), pesado `[100,100,180,150,220]` (380), especial `[95,95,95,95,160,260]` (380) | `/animations/attack_light/impact_frame`, `{ windup_ms: 260, cap_ms: 250 }` |
  | `windup_long` (ruleset) | `rulesets = [V1, V2]`, `V2.windup_heavy_ms = 280` | `/animations/attack_heavy/impact_frame`, `{ windup_ms: 300, cap_ms: 280, ruleset_version: 2 }` |
  | `anticipation` | pesado `frame_ms [300,100,100,150,220]`, `impact_frame: 1`, `frames [0,3,0,0,0]` | `/animations/attack_heavy/impact_frame`, `{ impact_frame: 1, min: 2 }` |
  | `impact_hold` | ligero `[60,60,40,170]` | `/animations/attack_light/frame_ms/2`, `{ ms: 40, min: 50 }` |
  | `recovery_short` | ligero `[60,60,90,100]` | `/animations/attack_light/frame_ms`, `{ recovery_ms: 190, min: 200 }` |
  | `reaction_slow` | `hit.frame_ms = [140,60,200]` | `/animations/hit/frame_ms/0`, `{ ms: 140, max: 134 }` |
  | `frame_empty` | celda 3 sin píxeles **y** los ataques sin cambios | `/frames/3` (sin `impact_short` en cascada) |
  | `frame_clipped` | píxel opaco en (0, 0) de la celda 2 | `/frames/2` |
  | `feet_off_pivot` | cuerpo de la celda 1 subido 5 filas | `/animations/idle/frames/1`, `{ row: 178 }`, una sola vez aunque `idle` use la celda 1 en los pasos 1 y 3 |
  | `height_range` | celda 0 con alto 50 (filas 134–183) | `/animations/idle/frames/0`, `{ height: 50 }` |
  | `impact_short` | brazo de la celda 3 hasta la columna 116 | tres errores, uno por ataque, en `/animations/<a>/frames/<impact>`, `{ reach: 20 }` |
  | `too_many_errors` | 101 frames con `pivot_x: 95` | 100 errores `pivot_position` y el último `{ code: 'guide.too_many_errors', path: '' }` |

  Además: también `duration_range` de `intro` (`[600,600,600]` → `max: 1600`), de `knockdown` (`[400,400,400,400]` → `max: 1500`, y con `V2.knockdown_ms = 1000` → `max: 1000`) y de `ko` (`[100,500,500,500]` → `max: 1500`); un test de que **cada** código de `GUIDE_CODES` tiene al menos un caso (como `covers every rule code` de `manifest.test.ts`); determinismo (dos llamadas dan resultados iguales con `toEqual`, y la entrada no se modifica); y `RangeError('guide.sheets_mismatch')` con una hoja de menos y con `data` de largo incorrecto.
- [ ] **`guide.windup_order`** (en el mismo archivo), con `data` exactos:
  - 120/300/320 (la referencia) pasa;
  - ligero 250 (`[125,125,90,120]`), pesado 300, especial 320 → un error en `/animations/attack_heavy/impact_frame`, `{ light_ms: 250, heavy_ms: 300, special_ms: 320, min_step: 100 }`;
  - ligero 120, pesado 380 (`[100,100,180,150,220]`), especial 320 → un error en `/animations/special/impact_frame`, `{ light_ms: 120, heavy_ms: 380, special_ms: 320, min_step: 100 }`;
  - ligero 250, pesado 340 (`[100,100,140,150,220]`), especial 320 → los dos errores, en ese orden (pesado < 250 + 100 y especial < pesado), sin `windup_short`;
  - límite exacto: ligero 200, pesado 300, especial 300 pasa (paso = 100 e igualdad pesado = especial).
  El `ref_a` de RF-B8 se prueba en la fase B.
- [ ] **RF-A4** (`guide-table.test.ts`): `windupCap` con `[V1]` da 400, 400 y 400; con `V2 = { windup_strike_ms: 300, windup_heavy_ms: 700, windup_special_ms: 350 }` y `[V1, V2]` da 300, 400 y 350; el orden de `rulesets` no cambia el resultado; `windupCap(m, [])` lanza. `windupBound` (vía la fila de la tabla): con `[V1]`, `attack_light` tiene `windup_ms { min: 100, max: 250 }` sin versión y `attack_heavy` `{ min: 250, max: 400 }` sin versión; con un `V2` de `windup_heavy_ms: 280`, `{ min: 250, max: 280, ruleset_version: 2 }`; con dos versiones empatadas en 280, la más baja.
- [ ] **RF-A5, Rust** (`cargo test -p content --test ruleset_timing`): `ruleset_timing_matches_its_golden` (bytes iguales), `timing_fields_are_sorted` y `every_listed_version_exists`. A mano, una vez, registrado en el PR: cambiar en local `windup_strike_ms` a 390 en `ruleset.rs` (sin commitear), comprobar que el test falla con el mensaje de regenerar, y revertir.
- [ ] **RF-A5, TS** (`guide-ruleset.test.ts`): el dorado real parsea; casos que fallan con `ruleset.invalid` y su `path`: una llave de más (`/versions/0/extra`), una de menos (`/versions/0/knockdown_ms`), una versión repetida (`/versions/1/ruleset_version`), versiones desordenadas, `versions: []` (`/versions`), un valor negativo, uno no entero, una llave de más en la raíz y una raíz que no es objeto (`''`). Más la atadura de la decisión 6 (la guía se puede cumplir con cada versión del dorado).
- [ ] **RF-A6** (`guide-table.test.ts`): `guideTable([V1])` da 12 filas en orden canónico con `required` correcto, y los valores efectivos de la spec: `intro.duration_ms.max` 1600, `hit`/`block`/`dodge` `duration_ms { min: 200, max: 1000 }` y `first_frame_max_ms` 134, `knockdown.duration_ms.max` 1500, `ko.duration_ms.max` 1500, `recovery_min_ms` 200 en los tres ataques, `win` sin `duration_ms`. Se comprueba con `toEqual` contra un objeto escrito en el test (es la tabla aprobada por game-designer; si cambia `GUIDE_LIMITS`, este test y la guía generada de la fase B se actualizan juntos). Un recorrido que asegura que la tabla no tiene texto: todos los valores hoja son números, booleanos o códigos de animación. `guideTable([])` lanza.
- [ ] **Placeholders** (decisión 8): `placeholders.test.ts` sin cambios y en verde (los dos pasan `validateManifest` con las 12 animaciones); en `guide-validate.test.ts`, `dummy_a` y `dummy_b` dan `guide.cell_size` en cada frame y `dummy_b` además `guide.windup_order`; y `guide-imports.test.ts` prueba que el runtime nunca llama a la guía.
- [ ] Sin capturas: no hay UI nueva ni cambia la escena (los tests de escena y la e2e de la arena no se tocan).

### Salida esperada de `just check`
Lo esperado es que pasen todos los pasos, en este orden: `check-web` (Prettier, ESLint, `svelte-check`, Vitest de web, `tsc` ×2, ESLint y Vitest del renderer, `sff-converter`, scripts, `check-*`) y después `check-rust` (fmt, clippy ×2, `_check-engine`, `_check-wasm`, `db-up`, `cargo test --workspace`, `_check-gen`, `_check-ts-rs-graph`). Lo que cambia respecto de main:
```
pnpm --filter @torneo/renderer exec vitest run
 ✓ test/windup-derivation.test.ts
 ✓ test/reaction-window.test.ts
 ✓ test/guide-limits.test.ts
 ✓ test/guide-ruleset.test.ts
 ✓ test/guide-table.test.ts
 ✓ test/guide-validate.test.ts
 ✓ test/guide-imports.test.ts
 ✓ test/eslint.test.ts            (casos nuevos de src/guide)
 Test Files  <N + 7> passed (<N + 7>)
…
     Running tests/ruleset_timing.rs (target/debug/deps/ruleset_timing-…)
running 3 tests
test every_listed_version_exists ... ok
test ruleset_timing_matches_its_golden ... ok
test timing_fields_are_sorted ... ok
test result: ok. 3 passed; 0 failed
```
Sin advertencias de `svelte-check` ni de clippy, y `_check-gen` sin diferencias (no cambia `protocol`). El PR lleva la salida real completa, no esta.

## Estimación de CI
Sin cambios en workflows. El PR dispara `policy`, `rust` (por `crates/content`) y `web` (por `packages/renderer`), y probablemente `e2e` según sus filtros de rutas. El costo agregado es de unos segundos: la propiedad de RF-A1 (< 2 s) y un test de Rust que recorre 65 536 versiones `const`.

## Desviaciones durante la implementación
1. **Corte de errores de `parseRulesetTiming`.** `RulesetError` solo admite `ruleset.invalid`, así que el corte de `ErrorSink` en `LIMITS.maxErrors` es un `ruleset.invalid` más con `path: ''`, no un código `ruleset.too_many_errors` nuevo. No agrega códigos a la spec; lo cubre un caso de `guide-ruleset.test.ts`.
2. **Pivote de las reglas de píxeles.** `feet_off_pivot` e `impact_short` miden contra el pivote estándar (96, 184), no contra el que declara el frame. Un frame con `guide.pivot_position` sigue pasando por las reglas de píxeles (no mide mal la celda ni está vacío) sin errores en cascada.
3. **`idle_distinct` sin `Buffer`.** Compara los bytes de las celdas de a pares (≤ 12 frames) en lugar de armar una llave: el núcleo puro no usa globals de Node (`tsc` de `src/` no tiene tipos de Node).
4. **Orden rojo → verde de RF-A1.** `reaction-window.test.ts` pasó contra el código anterior al refactor (corrido con una constante local de 200, porque `REACTION_WINDOW_MS` aún no existía). En `windup-derivation.test.ts`, los casos de `REACTION_WINDOW_MS`, de la derivación y de "sin literal `= 400`" fallaban antes del refactor por definición y pasaron después; los otros tres pasaron en ambos estados. Además, la propiedad corre el guion del spike y las tres doradas con **los tres** juegos de manifests (reapuntando los `asset`), no solo con los placeholders, y se comprobó que falla con una ventana de 201 ms sobre `MAX` (el borde es exacto).
5. **Verificación manual de RF-A5 (hecha).** Con `windup_strike_ms: 390` en local en `ruleset.rs`, `cargo test -p content --test ruleset_timing` falla con `ruleset-timing.json: bytes differ (first difference at Some(169)); a ruleset version changed or was added: regenerate with UPDATE_GOLDEN=1 …`. Revertido sin commitear; `crates/engine` no cambia en el diff.

## Revisión
### architect — 2026-10-28 — APPROVE
- Verifiqué contra la spec (con la respuesta del autor del 2026-10-27):
  - **RF-A1:** `windupMaxMs` se deriva de `TIMELINE_LIMITS.strikeGapMs − REACTION_WINDOW_MS`, sin ciclo porque `timeline/types.ts` no importa nada, y `MAX_WINDUP_MS`/`LOOKAHEAD_MS` lo leen. La propiedad de la coreografía se escribe antes del refactor y cubre el borde exacto de C2 (manifests `MAX` con impactos a 600).
  - **RF-A2:** `GUIDE_LIMITS` tiene las cifras aprobadas, incluidos `windupStepMs` 100 y `impact_frame` ≥ 3 en `special`. Los derivados no son campos. La frontera de runtime se prueba con un test de imports, no con `check-renderer-split`, y el motivo es correcto.
  - **RF-A3/A4:** están los 22 códigos de la tabla, `windup_order` con sus bordes (paso exacto de 100 e igualdad pesado = especial) y sin cascadas de píxeles. `validateGuide` lee de `guideTable`, así que no pueden divergir.
  - **RF-A5:** el dorado está en `crates/content`, solo con API pública, y el motor no se toca. El orden de las llaves sale del `struct`, más un test que lo vigila; `UPDATE_GOLDEN` no se puede activar en CI, el mensaje de fallo es explícito y `.prettierignore` está al día.
  - **RF-A6:** la tabla está hecha solo de números y códigos, sin texto.
  - **Pruebas:** cada RF tiene la suya, con `data` exactos. El plan nombra archivos, orden, riesgos y alternativas, y alguien sin contexto lo puede implementar.
- **006 (PR #46):** comprobé `git diff origin/main...origin/ta-21-faster-pacing`.
  - No toca `packages/renderer/src`, `crates/content` ni `.prettierignore`. En el renderer solo regenera `test/fixtures/golden/*.timeline.json`, que la propiedad de RF-A1 ya recorre.
  - En `ruleset.rs` cambia aserciones y suma `breather_*`, pero no los valores de `windup_*_ms` ni de `knockdown_ms`.
  - `TIMELINE_LIMITS.strikeGapMs` sigue en 600.
  - La estrategia (orden de merge indiferente para la fase A y, tras el rebase, el dorado tiene que pasar sin regenerar) es correcta.
- **Decisiones que me dejaste:**
  1. **`guide.too_many_errors`: sí.** Es el código del corte de `ErrorSink`, igual que `manifest.too_many_errors`. Va en `GUIDE_CODES`, cubierto por el test "cada código tiene su caso", y no se agrega a la tabla de reglas de la spec porque no es una regla de la guía.
  2. **`RangeError('guide.sheets_mismatch')`: sí.** Es una violación de la precondición de quien llama, no un defecto del set, con el precedente de `resolveAnimation`, y el mensaje es un código, sin texto (ADR-0007). Condición para `plan-b.md`: el chequeo y el importador garantizan la precondición antes de llamar. El IHDR coincide con `sheets[]` (`set.sheet_header`) y `data.length` sale de la decodificación, así que un set del repo nunca puede producir la excepción, y un test de la fase B lo demuestra.
  3. **`ruleset_version` en `guide.windup_long`: sí.** Solo va cuando el ruleset es el límite **estricto**. Ante un empate manda el tope estructural, sin versión, y entre versiones empatadas se reporta la más baja. Es lo que la spec quiere decir con "si lo limita el ruleset", y es determinista. En v1 ningún error lleva versión, como debe ser.
- Quedan como decisiones 9, 10 y 11 sin cambiar el cuerpo. No hay cambios pedidos.
