---
id: "003"
title: Motor de pelea v0 — fase A
type: plan
status: approved
risk: high
author: backend-engineer
reviewers: [architect]
approvals:
  - {by: architect, date: 2026-10-26}
linear: TA-3
depends_on: ["001"]
created: 2026-10-26
updated: 2026-10-27
---

# Plan: Motor de pelea v0 — fase A (desde spec.md 2026-10-26)

> [intent](./intent.md) · [spec](./spec.md) · ADRs [0001](../../decisions/0001-motor-propio-determinista.md), [0002](../../decisions/0002-commit-reveal-semilla.md), [0007](../../decisions/0007-i18n-desde-cero.md), [0010](../../decisions/0010-narrador-con-plantillas.md)

Alcance de la fase A: tipos del log en `crates/protocol` (RF1–RF3), `crates/engine` (RF4–RF11, RF20, RF21), `crates/engine-wasm` (RF19), dorados nativos y en WASI (RF16, RF18), chequeos del grafo y de pureza (RF5), `overflow-checks` y dependencias (RNF), y la parte de RF22 que toca a 003 (forma canónica del spec y el prerrequisito en `docs/architecture.md`). La fase B (`plan-b.md`) cubre `crates/content`, los personajes, `crates/sim` y el balance (RF12–RF15, RF17).

## Supuestos verificados (2026-10-26, solo lectura)
- **Lock:** `rand_chacha 0.9.0`, `rand_core 0.9.5`, `ppv-lite86 0.2.21`, `wasm-bindgen 0.2.129` (con `wasm-bindgen-macro`, `-macro-support`, `-shared`, `cfg-if`, `once_cell`, `rustversion` y `bumpalo`), `serde 1.0.229` y `serde_json 1.0.154` ya están en `Cargo.lock`. En una copia del workspace en el scratchpad agregué `crates/engine` y `crates/engine-wasm` con esas dependencias y `cargo metadata --offline`: el diff del lock son **solo** los dos `[[package]]` de ruta (`engine`, `engine-wasm`), sin ninguna versión de registro nueva.
- **Grafo de `engine` solo** (`cargo tree -p engine -e normal,features --target all`): no aparecen `getrandom`, `rand`, `tokio`, `ts-rs`, `tracing` ni `log`; de `rand_core` solo sale `feature "default"` (vacía en 0.9.5) y de `rand_chacha` ninguna feature. `rand_chacha` con `default-features = false` no activa `std`.
- **clippy 0.1.99:** en `crates/engine/clippy.toml`, `rand_core::SeedableRng::seed_from_u64` y `std::collections::HashMap` disparan `disallowed_methods`/`disallowed_types` aunque `rand_core` sea solo transitiva (el motor la usa como `rand_chacha::rand_core`). `allow-invalid = true` es una clave aceptada (clippy no rechaza el archivo). Con `cargo clippy -p engine` se toma el `clippy.toml` del crate y no el raíz.
- **`node:wasi` (Node 22.23.4):** un módulo mínimo que llama `proc_exit(3)` devuelve 3 con `returnOnExit: true`, y uno con `unreachable` lanza `WebAssembly.RuntimeError`, que el runner traduce a 101.
- **`cargo deny --offline check bans sources`** con `[graph] targets` ampliado a los dos wasm: `bans ok, sources ok` y las mismas 11 advertencias de duplicados que hoy (ninguna nueva).
- **Targets locales:** solo `aarch64-apple-darwin` y `x86_64-unknown-linux-musl`. Los wasm llegan con `rustup toolchain install` al cambiar `rust-toolchain.toml`.

## Decisiones de este plan (atienden las recomendaciones no bloqueantes de la revisión)
1. **`max_events` (security-reviewer):** el motor ni trunca en silencio ni entra en pánico. Antes de **empezar** cada acción, si `events.len() + EVENTS_PER_ACTION_MAX + EVENTS_TAIL_MAX > max_events` (3 por la acción y, como cota del cierre, 3 de momentos/KO, 2 de `low_hp`, 2 de `comeback` y `time_up` + `end`), `simulate` devuelve `Err(SpecError::EventLimit)` (código `event_limit`, en WASM `spec.event_limit`). Es inalcanzable con el ruleset v1: lo demuestra la cota analítica (decisión 2), y el test de extremos de RF21 lo confirma con el peor caso. Así RF4 ("la simulación no falla") se mantiene para todo ruleset que pase el test de cotas, y una versión futura mal calibrada falla de forma explícita. Agregar la variante a `SpecError` está dentro de RF4 ("por ejemplo").
2. **Cota analítica en el test de `Ruleset::v1()`:** con `interval_min = interval_base_ms − interval_per_speed_ms·stat_max` (800), el primer turno no antes de `intro_ms + interval_min/2` (3400) y el último inicio no después de `time_limit_ms − windup_special_ms` (84 900), cada lado hace como mucho `A = ⌊(84 900 − 3400) / (interval_min + windup_strike_ms)⌋ + 1 = ⌊81 500 / 1200⌋ + 1 = 68` acciones. Cota de eventos: `1 (intro) + 2·A·3 (attack, resolución, knockdown) + 2 (low_hp) + 2 (comeback) + 3 (decisive_crit, clutch_ko, ko) + 2 (time_up, end) = 418 < 512`. El test calcula `A` y la cota con los campos del ruleset (no con literales) y exige `cota < max_events`.
3. **Redacción de RF21:** se enmienda el texto (editorial, sin cambio de comportamiento) en el mismo PR: "cada lado hace como mucho 68 acciones; cada acción aporta como mucho 3 eventos (`attack`, la resolución y `knockdown`) y la pelea suma como mucho 10 eventos únicos (`intro`, 2 `low_hp`, 2 `comeback`, `decisive_crit`, `clutch_ko`, `ko`, `time_up`/`end`), así que la cota es 418 < 512; si se alcanzara, `simulate` devuelve `SpecError::EventLimit`". RF4 suma `event_limit` a la lista de ejemplo. Lo revisa architect con este plan.
4. **`clippy.toml` con rutas completas y `allow-invalid`:** `std::collections::HashMap`, `std::collections::HashSet`, `std::time::Instant`, `std::time::SystemTime` y `rand_core::OsRng` en `disallowed-types`; `rand_core::SeedableRng::{from_os_rng, try_from_os_rng, from_rng, try_from_rng, seed_from_u64}` en `disallowed-methods` (sumo `try_from_rng`, que es la variante falible de `from_rng`). `allow-invalid = true` en `OsRng`, `from_os_rng` y `try_from_os_rng`, que no existen cuando `os_rng` está apagado (`cargo clippy -p engine`, el target wasm). Cada entrada lleva `reason` en inglés (es configuración de desarrollo, no UI).
5. **`w_special_desperation_mul` (game-designer):** el "×2 si está desesperado" del peso de `special` es el campo `w_special_desperation_mul = 2` de `Ruleset`.
6. **Dorados = fixtures de 002.** Los `FightLog` de `crates/engine/tests/golden/log-*.json` son la única copia: la fase C de 002 (RF-C2) los lee desde Vitest con `fs` y una ruta relativa a la raíz del repo, sin duplicarlos. Formato: exactamente `canonical_json(log)` más un `\n` final (para que editores y `.editorconfig` no los toquen); el test compara `canonical_json(log) ++ b"\n"` con `include_bytes!`. Van en `.prettierignore` (Prettier los partiría en varias líneas) y con `-text` en `.gitattributes` (sin conversión de fin de línea).
7. **`FightLog::outcome()` devuelve `Option<(Option<Side>, EndReason)>`:** un log deserializado puede no tener `end`, y `protocol` no puede entrar en pánico ni usar `unwrap`. Para los logs del motor siempre es `Some`. Es una precisión de la firma de la spec.
8. **Rangos de validación en `Ruleset`:** además de `stat_budget`, los rangos de stats, personalidad, cantidad de habilidades y debilidades, y `power`/`cooldown` por `kind` son campos de `Ruleset` ("ninguna constante de juego vive fuera de `Ruleset`"). Los patrones de `slug` e `id` (longitud y caracteres) son formato, no juego, y quedan como constantes del módulo de validación.
9. **El jitter `roll(201)` se consume siempre**, también en la acción que produce el KO (paso 5 de RF7 sin excepción). No cambia el log y deja el orden de consumo sin casos especiales.

## Archivos que cambian
| Archivo | Cambio |
|---|---|
| `rust-toolchain.toml` | modificado: `targets = ["x86_64-unknown-linux-musl", "wasm32-unknown-unknown", "wasm32-wasip1"]` (RF18). |
| `scripts/__fixtures__/check-compose/valid/rust-toolchain.toml` | modificado: misma línea `targets`; el test de deriva de `check-compose.test.mjs` exige que la estructura coincida con el archivo real. |
| `deny.toml` | modificado: `[graph] targets` con los dos wasm (RF18). |
| `scripts/__tests__/deny-config.test.mjs` | modificado: caso nuevo, `[graph] targets` contiene los tres targets. |
| `Cargo.toml` | modificado: `members` suma `crates/engine` y `crates/engine-wasm` (se borra el comentario "still empty"); `[workspace.dependencies]` suma `engine = { path = "crates/engine" }`, `rand_chacha = { version = "=0.9.0", default-features = false }` y `wasm-bindgen = "=0.2.129"`; `[profile.release.package.engine] overflow-checks = true` (RNF); `[profile.dev.package.rand_chacha]` y `[profile.dev.package.ppv-lite86]` con `opt-level = 3` (ver Riesgos: velocidad de los tests en debug). |
| `Cargo.lock` | modificado: solo los `[[package]]` de `engine` y `engine-wasm` (verificado). |
| `scripts/check-crate-age.mjs` | modificado: `WORKSPACE_MEMBERS = ['engine', 'engine-wasm', 'protocol', 'server']` (`content` y `sim` entran en la fase B, cuando existan). |
| `scripts/__tests__/check-crate-age.test.mjs` | modificado: test nuevo, `WORKSPACE_MEMBERS` es exactamente el conjunto de `name` de los `[workspace].members` de `Cargo.toml` (falla si alguien agrega un crate y olvida el script). |
| `crates/protocol/src/fight.rs` | nuevo: `Side`, `AbilityKind`, `MomentKind`, `EndReason`, `FighterInfo`, `FightLog`, `FightEvent` como en la spec, más `FightLog::outcome()`; tests de forma JSON exacta por variante y de rechazo (RF1). |
| `crates/protocol/src/lib.rs` | modificado: `mod fight;` y `pub use fight::{...}`; el doc del crate menciona el log. |
| `packages/protocol-ts/src/generated/{Side,AbilityKind,MomentKind,EndReason,FighterInfo,FightLog,FightEvent}.ts`, `packages/protocol-ts/src/index.ts` | generados por `just gen` (nunca a mano). |
| `crates/engine/Cargo.toml` | nuevo: deps `protocol` (sin `ts`), `serde`, `serde_json`, `rand_chacha`; `[lints] workspace = true`; sin dev-dependencies (el test WASI compila igual). |
| `crates/engine/clippy.toml` | nuevo: `allow-unwrap-in-tests`, `allow-expect-in-tests` (repetidas del raíz, no se combinan) y las listas de la decisión 4 (RF5). |
| `crates/engine/src/lib.rs` | nuevo: `#![deny(clippy::float_arithmetic, clippy::indexing_slicing, clippy::panic, clippy::unreachable, clippy::todo, clippy::unimplemented)]`, docs del contrato de pureza y reexports (RF4, RF5). |
| `crates/engine/src/seed.rs` | nuevo: `Seed([u8; 32])`, `from_bytes`, `Debug` manual `Seed(<redacted>)`, sin `Clone`/`Serialize`/`Display`/hex ni getter; acceso `pub(crate)` solo para sembrar el PRNG (RF4, RF20). |
| `crates/engine/src/rng.rs` | nuevo: `Roller` sobre `ChaCha20Rng::from_seed`, única función `roll(NonZeroU32) -> u32` por rechazo en `u64` (RF6). |
| `crates/engine/src/ruleset.rs` | nuevo: `Ruleset` con campos privados y getters (declarados juntos con un `macro_rules!` para no duplicar la lista), `for_version(u16) -> Option<Ruleset>`, `pub(crate) const fn v1()`, `interval_min()`; tests de cotas (decisión 2) (RF4, RF10). |
| `crates/engine/src/spec.rs` | nuevo: `CharacterSpec`, `Stats`, `Personality`, `Ability`, `MoveKind { Heavy, Special }`, `Element` (6 valores), todos `Serialize`/`Deserialize` con `deny_unknown_fields`; `element: Option<Element>` sin `skip_serializing_if` (sale `null`); `CharacterSpec::validate(&Ruleset)`; `SpecError` con `code()` y `SpecError::ALL` (RF4, RF12 en lo que toca al motor). |
| `crates/engine/src/encode.rs` | nuevo: `canonical_json`, `canonical_spec_json` con `serde_json::to_vec` y `map_err` a `EncodeError` (RF4, RF11). |
| `crates/engine/src/sim.rs` | nuevo: estado de la pelea, elección de quién actúa, acción, defensa, daño, knockdown, `low_hp`, momentos, fin y guarda de `max_events` (RF7–RF9). |
| `crates/engine/tests/common/mod.rs` | nuevo: carga de fixtures con `include_str!`, `derive_seed(i)` (semilla raíz fija con los 4 primeros bytes XOR `i` en LE) y `assert_invariants(&FightLog, &Ruleset)` con todas las invariantes de RF2. |
| `crates/engine/tests/fixtures/{striker,trickster,tank,turtle}.json` | nuevo: 4 specs en forma motor, válidos, desacoplados de `content/` (RF16). `turtle` es defensivo extremo (hp 100, defense 100, attack 1) para las decisiones. |
| `crates/engine/tests/golden.rs` | nuevo: vectores (a), digest FNV-1a (b), specs canónicos e ida y vuelta (c); `UPDATE_GOLDEN=1` solo en nativo; test `#[ignore]` de búsqueda de semillas (RF16). |
| `crates/engine/tests/golden/log-*.json`, `spec-*.json`, `digest-1000.txt` | nuevo: generados por `UPDATE_GOLDEN=1` y commiteados (RF16, fixtures de 002). |
| `crates/engine/tests/properties.rs` | nuevo: 1000 semillas por par ordenado de fixtures (16 pares, espejos incluidos), determinismo y RF2; extremos de RF21. |
| `crates/engine-wasm/Cargo.toml` | nuevo: `crate-type = ["cdylib", "rlib"]`; deps `engine`, `serde_json`, `wasm-bindgen`; tabla `[lints]` explícita igual a la del workspace salvo `unsafe_code = "deny"` (RF19). |
| `crates/engine-wasm/src/lib.rs` | nuevo: los mismos `deny` de RF5, `WasmError` con sus códigos, `run(...) -> Result<String, WasmError>` (Rust puro) y la única función exportada `simulate` con `#[allow(unsafe_code)]` que mapea a `JsError::new(code)` (RF19). |
| `crates/engine-wasm/tests/codes.rs` | nuevo: cada código de error sobre `run` en nativo; ningún error contiene texto de `serde_json`; cobertura de `SpecError::ALL` (RF19). |
| `scripts/check-engine.mjs` | nuevo: funciones puras `parseCargoTree`, `checkEngineGraph`, `checkWasmSources`, `parseLintTables`, `checkWasmLints`; el `main` corre `cargo tree` y lee los archivos (RF5, RF19, RF20). |
| `scripts/__tests__/check-engine.test.mjs`, `scripts/__fixtures__/check-engine/*` | nuevo: casos positivos y negativos (ver Pruebas). |
| `scripts/wasi-runner.mjs` | nuevo: `wasiOptions(wasmPath, args)` y `runWasi(...)` con `node:wasi` (`version: 'preview1'`, `env: {}`, sin `preopens`, `returnOnExit: true`); trap → 101 (RF18). |
| `scripts/__tests__/wasi-runner.test.mjs` | nuevo: módulos wasm mínimos escritos como bytes en el test (sin binarios commiteados). |
| `justfile` | modificado: recetas `_check-engine` y `_check-wasm` dentro de `check-rust` (antes de `db-up`, no necesitan Docker); `_check-ts-rs-graph` se extiende a `engine` y `engine-wasm`. |
| `.github/workflows/ci.yml` | modificado: el filtro `rust` suma `scripts/check-engine.mjs` y `scripts/wasi-runner.mjs` (los corre `check-rust`). Nada más cambia: el job `rust` ya instala Node y la toolchain desde `rust-toolchain.toml`. |
| `.prettierignore` | modificado: `crates/engine/tests/golden/`. |
| `.gitattributes` | nuevo: `crates/engine/tests/golden/* -text`. |
| `docs/architecture.md` | modificado: firma `-> Result<FightLog, SpecError>`, crates `engine`/`engine-wasm` y dirección `protocol ← engine ← engine-wasm`, serialización canónica, y RF22 más "sin `timeline_version` 2 no se publican peleas reales" (no bloqueante 2 de product-owner) en "Prerrequisitos del primer despliegue a producción". |
| `docs/features/003-motor-de-pelea/spec.md` | modificado (editorial, decisión 3): texto de RF21 y `event_limit` en RF4. |
| `docs/features/003-motor-de-pelea/plan-a.md` | este plan; "Desviaciones" si la implementación se aparta. |

Sin cambios: `apps/server/**` (incluido el `Dockerfile`: `rustup toolchain install` descargará también los dos targets wasm en la etapa `chef`, decenas de MB en una capa cacheada; lo acepta la preocupación 6 y evita tocar `check-compose.mjs`), `apps/web/**`, `content/**`, `clippy.toml` raíz.

## Orden de trabajo
Un commit por paso (`feat(engine): … (TA-3)`, `feat(protocol): …`, `build: …`, `ci: …`, `docs(003): …`), en la rama `ta-3-motor-de-pelea`.

1. **Toolchain, workspace y lock (el paso de infraestructura más delicado).**
   - `rust-toolchain.toml` y su copia en `scripts/__fixtures__/check-compose/valid/`; `deny.toml` y su test; `rustup toolchain install` en el repo.
   - `Cargo.toml` (miembros, dependencias, perfiles) y los dos crates vacíos con `Cargo.toml` y `lib.rs` (solo lints).
   - `cargo metadata --format-version 1 >/dev/null` (sin `--locked`, una sola vez, para que cargo escriba los dos `[[package]]` de ruta). `git diff Cargo.lock` tiene que mostrar **solo** esos dos bloques; si aparece cualquier paquete de registro, se para y se revisa la feature que lo trajo.
   - `just crate-age` (red) **antes** del primer build; `WORKSPACE_MEMBERS` y su test.
   - `just deny`. A partir de aquí todo comando cargo usa `--locked`.
2. **`protocol`** (paralelizable con 3): `fight.rs`, tests de forma, `just gen`, commit de `packages/protocol-ts`. `cargo clippy -p protocol --features ts` en verde.
3. **Scripts** (paralelizable con 2): `wasi-runner.mjs` y `check-engine.mjs` con sus tests y fixtures. Se prueban con `pnpm exec vitest run --config scripts/vitest.config.mjs`, sin depender del motor.
4. **Motor, piezas sin simulación:** `seed.rs`, `rng.rs`, `ruleset.rs` (con el test de cotas), `spec.rs` (validación y `SpecError`), `encode.rs`, `clippy.toml`. Verificación manual del `clippy.toml` (ver Riesgos) antes de seguir.
5. **Motor, simulación:** `sim.rs` y `simulate`, con tests unitarios por fórmula (ver Pruebas). Detalles fijados:
   - **Acción** (atómica, en `next_at` del atacante): elección por peso acumulado (`strike`, luego habilidades disponibles en orden del archivo; los pesos se suman con `NonZeroU32::saturating_add` partiendo del de `strike`, que es ≥ 100, así que `roll` nunca recibe 0); `attack` en `t = next_at`; resolución en `t_res = t + windup(kind)`.
   - **Enfriamiento:** contador por habilidad; disponible si es 0; tras elegir, se decrementan los demás contadores > 0 y el usado pasa a `cooldown`.
   - **Daño:** golpe `d = base·(85 + roll(31))/100`, desesperación ×5/4, `d_nc = d`, crítico ×3/2 sobre `d`, debilidad ×5/4 sobre `d` y `d_nc`, `max(·, 1)` a los dos; bloqueo: varianza, debilidad, ×3/10 (puede dar 0). Aplicado `min(d, hp_antes)`.
   - **Tras la resolución, en este orden de eventos:** resolución → `knockdown` (crítico, `hp > 0` y `damage·1000 ≥ knockdown_min_permille·max_hp_def`) → `low_hp` del objetivo (primera vez, `hp > 0`) → `comeback` (se evalúa con las marcas previas de "estuvo ≥ 250 ‰ abajo" y después se actualizan las marcas de ambos lados) → si `hp = 0`: `decisive_crit`, `clutch_ko`, `ko`, y `end` en `ko.t + end_ms`.
   - **Tiempos:** `next_at_atk = t_res + interval + roll(201)`; `next_at_def = max(next_at_def, t_res + recovery_ms)`; knockdown alarga ambos según la tabla.
   - **Turno:** menor `next_at`; empate → el que no actuó último; si nadie actuó, la iniciativa. Si `next_at + windup_special_ms > time_limit_ms` para el que toca, `time_up` en `time_limit_ms`, `end` en `time_limit_ms + end_ms` con `decision`/`draw` por `hp·1000/max_hp`.
   - Un bloqueo cuyo chip deja `hp = 0` también es KO (RF9: "cuando el hp llega a 0"); el `ko` sigue a ese `block`. Ver Riesgos (002).
6. **Tests de integración del motor:** fixtures, `common/mod.rs`, `properties.rs`; después `golden.rs`: con el test `#[ignore] search_golden_seeds` (imprime con `eprintln!` bajo un `#[allow(clippy::print_stderr)]` local; `cargo test -p engine --test golden -- --ignored --nocapture`) se eligen los índices de semilla de cada vector, se fijan en la tabla y se generan los dorados con `UPDATE_GOLDEN=1 cargo test -p engine --test golden`. Se revisan a mano dos logs (uno de KO, uno de decisión) contra las fórmulas antes de commitear.
7. **`engine-wasm`:** `run`, `simulate` exportada, `tests/codes.rs`. `cargo clippy -p engine -p engine-wasm --target wasm32-unknown-unknown --locked -- -D warnings`.
8. **Recetas y CI:** `_check-engine`, `_check-wasm`, extensión de `_check-ts-rs-graph`, filtro `rust` de `ci.yml`, `.prettierignore`, `.gitattributes`. `node scripts/check-workflows.mjs` en verde.
9. **Docs:** `docs/architecture.md`, enmienda editorial de RF21/RF4 en la spec, desviaciones de este plan.
10. **`just check`** completo, con la salida pegada en el PR; PR "feat(engine): fight engine v0, phase A (TA-3)".
11. **Al fusionar:** pedir formalmente en Linear (issue de 002) la enmienda de la spec de 002 para `timeline_version` 2 (preocupación 7), con el enlace a los dorados `log-decision-*.json`/`log-draw.json` como casos que v1 no expresa.

### Recetas nuevas (texto exacto a implementar)
```just
# RF5, RF19, RF20: engine graph and features, engine-wasm unsafe and lints
_check-engine:
    node scripts/check-engine.mjs

# RF18: engine and bindings compile for the browser; the same goldens pass in WASI
_check-wasm:
    #!/usr/bin/env bash
    set -euo pipefail
    for target in wasm32-unknown-unknown wasm32-wasip1; do
        if ! rustup target list --installed | grep -qx "$target"; then
            echo "missing target $target: run \`rustup toolchain install\` in the repo" >&2
            exit 1
        fi
    done
    cargo clippy -p engine -p engine-wasm --target wasm32-unknown-unknown --locked -- -D warnings
    CARGO_TARGET_WASM32_WASIP1_RUNNER="node --disable-warning=ExperimentalWarning {{justfile_directory()}}/scripts/wasi-runner.mjs" \
        cargo test -p engine --test golden --target wasm32-wasip1 --locked -- --nocapture
```
Extensión de `_check-ts-rs-graph` (nota 1 de la revisión de architect), después de los dos chequeos existentes:
```just
    for crate in engine engine-wasm; do
        if cargo tree -p "$crate" -e normal --locked --prefix none | grep -qE '^(ts-rs|tokio) '; then
            echo "$crate pulls ts-rs or tokio (003 RF5)" >&2
            exit 1
        fi
    done
```
`check-rust` queda: `fmt` → clippy workspace → clippy `protocol --features ts` → `_check-engine` → `_check-wasm` → `db-up` → `cargo test --workspace` → `_check-gen` → `_check-ts-rs-graph`. El runner de cargo se parte por espacios: la ruta absoluta del repo no puede tener espacios (hoy no los tiene; el mensaje de error de cargo lo haría evidente).

### Fixtures de `FightLog` para 002 (contrato)
- **Dónde:** `crates/engine/tests/golden/log-<nombre>.json`. Nombres estables: `log-ko-decisive-crit.json`, `log-ko-knockdown.json`, `log-ko-comeback.json`, `log-ko-clutch.json`, `log-decision-a.json`, `log-decision-b.json` y, si la búsqueda la encuentra, `log-draw.json`. Cada vector de KO cubre además `low_hp`; entre todos cubren las 11 variantes de `FightEvent` y los tres `MomentKind`.
- **Cómo se producen:** solo el motor, con `UPDATE_GOLDEN=1 cargo test -p engine --test golden` en nativo. En WASI la variable no existe (`env: {}`) y el código de escritura está bajo `#[cfg(not(target_family = "wasm"))]`; si `UPDATE_GOLDEN` y `CI` están definidas a la vez, el test falla (CI nunca regenera).
- **Cómo se commitean:** en el mismo commit que el cambio del motor que los altera (RF10), con el diff revisado en el PR. Mientras `ruleset_version = 1` sea preproducción, regenerar está permitido; el PR lo declara.
- **Cómo los consume 002:** Vitest de la fase C los lee con `readFileSync(new URL('../../../../crates/engine/tests/golden/…', import.meta.url))` y `JSON.parse` (el `\n` final no molesta), tipados con `FightLog` de `@torneo/protocol`. El filtro `web` de CI ya incluye `crates/engine/**`, así que un cambio de dorados vuelve a correr esos tests. Los de `decision`/`draw` deben dar `adapter.unsupported_ending` hasta que exista la v2.

## Riesgos
- **Velocidad de los tests en debug** (ChaCha y `ppv-lite86` sin optimizar son lentos; `properties.rs` hace 16 × 1000 × 2 simulaciones, más 1000 del digest en nativo y en WASI) → `opt-level = 3` para `rand_chacha` y `ppv-lite86` en `[profile.dev.package]` (no cambia resultados: la aritmética es entera y `overflow-checks` del motor no se toca en dev). Se mide `cargo test -p engine` al terminar el paso 6; si pasa de ~15 s en la Mac del owner, se reduce `properties.rs` a 1000 semillas por par **no ordenado** más espejos (10 pares) y se registra como desviación.
- **`clippy.toml` que no dispara** (una ruta mal escrita con `allow-invalid` pasa en silencio) → verificación manual en el paso 4 con un archivo temporal no commiteado que usa cada tipo y método prohibido; `cargo clippy -p engine` y `cargo clippy --workspace` (este último con `os_rng` unificado) tienen que fallar en cada uso. El resultado se pega en el PR. Las entradas sin `allow-invalid` (`HashMap`, `seed_from_u64`, …) ya se comprobaron.
- **Un bump o un cambio de features mete versiones en el lock** → paso 1: diff del lock limitado a paquetes de ruta y `just crate-age` antes de compilar; RF10 exige que un bump de `rand_chacha`/`serde`/`serde_json` pase los dorados sin regenerar.
- **`node:wasi` es experimental** → el runner es pequeño, su API se usa en un solo lugar y sus tests con módulos mínimos detectan un cambio de comportamiento en el Node fijado; la advertencia se silencia con `--disable-warning=ExperimentalWarning`.
- **libtest en `wasm32-wasip1`** (sin hilos, `panic = abort`) → `golden.rs` no usa `should_panic` ni hilos; un fallo de aserción aborta el binario y el runner lo convierte en 101, así que cargo lo reporta como fallo. Si un fallo deja poca información, se reproduce en nativo con los mismos dorados.
- **Prettier o un editor alteran los dorados** → `.prettierignore`, `-text` en `.gitattributes` y `\n` final explícito; el test compara bytes, así que cualquier alteración falla en `just check`.
- **Toolchain local sin targets** (quien ya tenía 1.99.1 instalado) → `_check-wasm` falla con el mensaje `rustup toolchain install`; se menciona en el PR. La línea de `CLAUDE.md` que describe `check-rust` debería sumar `_check-engine` y `_check-wasm`, pero `CLAUDE.md` lo cambia solo el owner: lo propongo en el PR y no lo edito.
- **Imagen del server más pesada en la etapa builder** (targets wasm) → aceptado por la preocupación 6; el binario final no cambia. El job `images` corre en este PR porque cambia `rust-toolchain.toml`.
- **KO por chip de bloqueo frente a la `Timeline` de 002** (un `block` con `hp = 0` seguido de `ko`) → es lo que dice RF9; lo reporto a 002 junto con el pedido de la v2 para que su validador acepte `ko` tras `strike` con `result: block`. Si 002 no puede, es una enmienda de forma de esta spec (no se cambia en este PR).
- **Divergencia entre el modelo Python del game-designer y el motor** → no afecta a la fase A (no hay criterios de balance aquí); `sim` la mide en B.
- **`SpecError::EventLimit` como error de "spec"** → nombre poco preciso para un fallo del ruleset; se acepta para no cambiar la firma de RF4, documentado en el doc comment de la variante.

## Alternativas descartadas
- **Fixtures para 002 en `packages/protocol-ts/fixtures/` o `packages/renderer/fixtures/`:** duplican los dorados y ponen archivos del motor en paquetes generados o de otra feature (`protocol-ts` no se edita a mano). Una sola copia en `crates/engine/tests/golden/` evita la deriva.
- **Generar los fixtures de 002 con `engine-wasm` en Vitest:** necesita glue JS y `wasm-bindgen-cli`, que 003 deja fuera (RF19).
- **Truncar el log al llegar a `max_events` o forzar `time_up`:** un log válido pero falso; rechazado por la recomendación de security-reviewer.
- **Nuevo `SimError` y `Result<FightLog, SimError>`:** cambia la firma aprobada de RF4 por un caso inalcanzable.
- **Campos `pub` en `Ruleset`:** un llamador podría mutar una copia y simular con valores que no son de ninguna versión.
- **`proptest` para las propiedades:** sería un crate nuevo; la spec pide semillas derivadas sin dependencias.
- **Derivar semillas con `ChaCha20Rng` en los tests:** innecesario; XOR del índice sobre una raíz fija es trivial, determinista y no consume el PRNG del motor.
- **Quitar los targets wasm del `Dockerfile` (por ejemplo, `rustup toolchain install` con una lista explícita):** obligaría a enseñar a `check-compose.mjs` otra forma de instalar la toolchain para ahorrar una descarga cacheada.
- **Agregar `content` y `sim` a `WORKSPACE_MEMBERS` ya:** el test nuevo exige que coincida con `[workspace].members`, así que entran con sus crates en la fase B.

## Pruebas (cómo se demuestra que funciona)
- [ ] `just check` en verde, salida pegada en el PR.
- [ ] `just crate-age` y `just deny` en verde tras el paso 1; diff de `Cargo.lock` solo con `engine` y `engine-wasm`.
- [ ] **RF1:** `fight.rs` tests: JSON exacto de cada una de las 11 variantes de `FightEvent`, de `FightLog`/`FighterInfo` y de los 4 enums; rechazo de `type` desconocido, de un campo que falta y de un `Side` fuera de `a|b`; `outcome()` con y sin `end`. `just gen` produce los 7 `.ts` y `_check-gen` no ve diferencias; `gen-protocol-ts` falla si apareciera `bigint` (ya existente).
- [ ] **RF2:** `properties.rs` aplica `assert_invariants` a cada log (16 pares × 1000 semillas): `intro` en 0, `t` no decreciente, un solo `end` al final con `end.t ≤ time_limit_ms + end_ms`, ≤ 512 eventos, cadena de `hp`/`damage` por lado, `hp = 0` solo antes de `ko`, reglas de `ko`/`time_up`/`winner`, primera resolución ≥ 2000, resoluciones separadas ≥ 600, `knockdown` con `hit` en el mismo `t` y 800–3000 ms, sin resoluciones dentro de la caída. `golden.rs` aplica las mismas invariantes a sus vectores (también en WASI).
- [ ] **RF3:** test que serializa los logs de los vectores y comprueba que todo `String` del log es un `slug` o un `ability` válido; RF20 cubre la semilla.
- [ ] **RF4:** tests de API: `for_version(1)` es `Some`, `for_version(0|2)` es `None`; `simulate` con spec inválido devuelve el `SpecError` esperado; ida y vuelta de `CharacterSpec`; `deny_unknown_fields` rechaza un campo de más en cada struct anidado; `SpecError::ALL` contiene cada variante una vez y `code()` es único y `snake_case`. Que `Seed` no tenga `Serialize`/`Display`/`LowerHex`/`Clone` se comprueba con aserciones de compilación negativas mediante un trait auxiliar en el test (`impls!`-style con autoref, sin crates).
- [ ] **RF5:** los `deny` de `lib.rs` y `clippy.toml` se ejercitan con `cargo clippy` (verificación manual del paso 4 pegada en el PR); `check-engine.test.mjs`: pasa con la salida real de `cargo tree` capturada como fixture y falla con `getrandom`, `rand`, `tokio`, `ts-rs`, `tracing` o `log` en el grafo, y con `rand_core feature "os_rng"`, `rand_chacha feature "serde"` o `rand_chacha feature "std"`; no confunde `rand_chacha`/`rand_core` con `rand`.
- [ ] **RF6:** unit de `roll`: `n = 1` devuelve 0 sin rechazar; con un `RngCore` de prueba que devuelve valores elegidos, `x ≥ zona` se descarta y `x < zona` da `x mod n`; con `n = 3`, `zona = 4 294 967 295`; con `n = 2³¹ + 1`, el rechazo ocurre en la mitad superior. El motor no usa `rand::distributions` (no depende de `rand`; lo comprueba `check-engine`).
- [ ] **RF7:** unit con un `Roller` instrumentado que registra cada `n` pedido: una pelea corta reproduce la secuencia `2, (peso, 1000, [31], [1000], 201)*` con `31` solo en golpe o bloqueo y el segundo `1000` solo en golpe.
- [ ] **RF8:** unit de selección de turno (menor `next_at`, empate al que no actuó último, iniciativa), de recuperación tras golpe, bloqueo y esquiva, de knockdown alargando ambos `next_at`, y del tope `next_at + windup_special_ms > time_limit_ms`.
- [ ] **RF9:** unit de la regla de resultado: KO gana el atacante y `end = ko.t + end_ms`; por tiempo, mayor `hp·1000/max_hp` con `decision`; igualdad (incluido `max_hp` distinto con el mismo permil) da `draw` y `winner = null`. Si la búsqueda encuentra una semilla de empate, también `log-draw.json`.
- [ ] **RF10:** `golden.rs` (a) y (b) fallan ante cualquier cambio de bytes; el PR no regenera dorados salvo en el commit que los crea.
- [ ] **RF11:** dorados comparados byte a byte; test de que `canonical_json` no tiene espacios fuera de cadenas ni saltos de línea, y de que los campos salen en orden de declaración.
- [ ] **RF16:** (a) ≥ 6 vectores con `ko`, `knockdown` y `decision` (más `draw` o el unit de RF9); (b) digest FNV-1a de 64 bits de 1000 peleas contra `digest-1000.txt`; (c) `spec-*.json` de los 4 fixtures e ida y vuelta.
- [ ] **RF18:** `_check-wasm` dentro de `check-rust`: clippy en `wasm32-unknown-unknown` y `golden` en `wasm32-wasip1` con los mismos archivos. `wasi-runner.test.mjs`: `wasiOptions` tiene `env: {}`, ningún `preopens` y `args[0]` = nombre del módulo; un módulo que hace `proc_exit(3)` devuelve 3, uno que llega a `unreachable` devuelve 101, uno que termina normal devuelve 0, y un archivo que no es wasm devuelve 1 con mensaje.
- [ ] **RF19:** `tests/codes.rs` sobre `run`: `input.too_large` para cada argumento de texto > 8192 bytes (y que se rechaza sin parsear: un JSON inválido de 9 KiB da `input.too_large`, no `spec.invalid_json`), `seed.invalid_hex` (63 y 65 caracteres, un carácter no hex), `ruleset.unknown`, `spec.invalid_json` (campo de más, JSON roto, forma de archivo con `archetype`), un `spec.<código>` por cada `SpecError::ALL` con un spec construido para producirlo (salvo `event_limit`, que se comprueba por la tabla de códigos), y el log OK coincide con `canonical_json`. Ningún mensaje contiene texto de `serde_json` (todos cumplen `^[a-z_]+\.[a-z_]+$`). `log.encode_failed` se cubre por la tabla de códigos (no es provocable). `check-engine.test.mjs` falla con `unsafe` en `src`, con dos `allow(unsafe_code)` y con un lint distinto, faltante o extra en `[lints]`, y pasa con `unsafe_code` de `forbid` a `deny`.
- [ ] **RF20:** `format!("{:?}", seed) == "Seed(<redacted>)"`; ningún dorado contiene los bytes de la semilla en hex (test que busca la semilla de cada vector en su log); `check-engine` sin `tracing`/`log`.
- [ ] **RF21:** `properties.rs` alimenta `simulate` con specs inválidos (cada código de `SpecError`) → `Err`; extremos válidos (todos los stats en 1; stats en el presupuesto exacto en varias combinaciones; personalidad en 0 y en 100; 4 especiales de `power 100`, `cooldown 2`; 2 debilidades) → `Ok` y RF2; todos en 100 → `Err(budget_exceeded)`. Peor caso de eventos (dos lados con `speed 100`, `defense 100`, `hp 100`): `events.len() ≤ 418`. Test de `Ruleset::v1()` con las cinco cotas de la spec, la cota analítica (< `max_events`), divisores no nulos, `interval_base_ms > interval_per_speed_ms·stat_max`, daño máximo y `max_hp` dentro de `u16`, y `time_limit_ms + end_ms` dentro de `u32`.
- [ ] **RF22 (parte de 003):** `canonical_spec_json` con dorado e ida y vuelta (RF16 c) y el prerrequisito escrito en `docs/architecture.md`.
- [ ] **RNF:** `overflow-checks = true` en `[profile.release.package.engine]` (revisión del diff); dependencias sin versiones nuevas (paso 1).

## Estimación de CI
- `changes`/`policy`: sin cambios de duración (el filtro solo suma dos rutas).
- `rust`: +2 targets en `rustup toolchain install` (~15 s), clippy wasm32 (~40 s en frío, menos con `rust-cache`, que guarda `target/` de todos los targets), build y corrida de `golden` en WASI (~40 s en frío), `check-engine` (~2 s) y los tests nativos del motor (~15 s). Unos **+2 a 3 min** en frío y **+1 min** con caché, dentro del `timeout-minutes: 20`.
- `web`/`e2e`: corren porque cambian `scripts/**`, `crates/engine/**` y `packages/protocol-ts/**`; +3 archivos de Vitest, segundos.
- `images`: corre una vez en este PR por `rust-toolchain.toml` y `Cargo.lock`; la etapa `chef` descarga los targets wasm (+~20 s en frío).

## Desviaciones durante la implementación
1. **`properties.rs` usa 10 pares, no 16.** Con los 16 pares ordenados, el test tardaba 17,4 s en debug (el costo es la simulación, ~0,5 ms por pelea, no las invariantes). Se aplicó la salida prevista en Riesgos: 1000 semillas por par **no ordenado** más los espejos (10 pares), 11,0 s. Los dos lados atacan en todas las peleas, así que las rutas de `a` y `b` se siguen ejercitando; el digest de 1000 peleas sí recorre los 16 pares ordenados.
2. **Peor caso de eventos con `hp 98`.** "`speed 100`, `defense 100`, `hp 100`" suma 300 sin dejar los mínimos de `attack` y `special` (1). Se usa `hp 98, attack 1, defense 100, speed 100, special 1`; el test exige `events.len() ≤ max_events_bound() = 418` en 1000 semillas.
3. **`.prettierignore` y `.gitattributes` entraron en el commit de los dorados** (paso 6) y no en el paso 8, para que cada commit pase Prettier.
4. **Campos de `Ruleset` que la tabla no nombra:** `variance_min = 85` y `variance_span = 31` (el `85 + roll(31)` de "Orden del daño"). Con ellos, más `w_special_desperation_mul` (decisión 5) y los rangos de validación (decisión 8), ninguna constante de juego vive fuera de `Ruleset`. `Ruleset` expone además `interval_min()` y `max_events_bound()` (la cota analítica de la decisión 2, que el test exige igual a 418).
5. **Ayudas en `protocol` sin cambio de formato:** `Side::opponent()` y `FightEvent::t()`. Son métodos, así que no cambian el JSON ni los `.ts` generados.
6. **`SpecError` (RF4 da ejemplos):** `slug_invalid`, `stat_out_of_range`, `budget_exceeded`, `personality_out_of_range`, `ability_count`, `ability_id_invalid`, `ability_id_duplicate`, `power_out_of_range`, `cooldown_out_of_range`, `weakness_count`, `weakness_duplicate` y `event_limit`. `EncodeError` es una struct unitaria con el código `encode_failed`.
7. **Verificación manual de `clippy.toml` (Riesgos), con un módulo temporal no commiteado:**
   - `cargo clippy -p engine` marcó `HashMap` y `HashSet` (2 usos cada uno), `Instant`, `SystemTime`, `from_rng`, `try_from_rng` y `seed_from_u64`. Con `os_rng` apagado, `from_os_rng`, `try_from_os_rng` y `OsRng` ni siquiera compilan (`E0599`/`E0425`).
   - `cargo clippy --workspace` (con `os_rng` unificado) marcó además `from_os_rng` y `try_from_os_rng`, y `rand_core::OsRng` en posición de tipo (`use`, parámetro, retorno).
   - **Límite de clippy:** `disallowed-types` no marca `OsRng` usado como **valor** (`let g = OsRng;`). La guarda real es que `os_rng` está apagado en el grafo de `engine` solo (`check-engine.mjs`), así que ese código no compila con `-p engine` ni para wasm.
8. **Lints en los tests (nota 3 de la revisión):** los `deny` de `lib.rs` no exigieron ningún `#[allow]` en los `#[cfg(test)]` de `src/`. `allow-unwrap-in-tests` **no** cubre las funciones auxiliares de `tests/common/mod.rs` (lo comprobé: un `unwrap()` en una función que no es `#[test]` falla clippy), así que usan `unwrap_or_else(|e| panic!(…))`, que es válido en los crates de test.
9. **Hallazgo sobre `wasm-bindgen 0.2.129` (sin desviación):** el shim generado no dispara `unsafe_code` en el crate del usuario (viene de la expansión de una proc-macro externa, donde rustc no reporta el lint). Compilé `engine-wasm` para `wasm32-unknown-unknown` sin el `#[allow(unsafe_code)]` y también con `unsafe_code = "forbid"`, y los dos pasan (el `.wasm` exporta `simulate`). Se mantiene el diseño aprobado de RF19 (`deny` y un único `allow`, vigilados por `check-engine.mjs`); endurecerlo a `forbid` sería una enmienda de la spec que decide architect. Superado por la desviación 13.
10. **`seed_hex` acepta mayúsculas y minúsculas.** RF19 solo fija la longitud (64 caracteres); un carácter no hexadecimal, o una longitud distinta, dan `seed.invalid_hex`.
11. **Vectores dorados elegidos con `search_golden_seeds`:** `log-ko-decisive-crit` (striker contra trickster, semilla 1), `log-ko-knockdown` (trickster contra tank, 0), `log-ko-comeback` (striker contra tank, 4), `log-ko-clutch` (trickster contra trickster, 1), `log-decision-a` (tank contra turtle, 0), `log-decision-b` (turtle contra tank, 1) y `log-draw` (turtle contra turtle, 53). Se descartó tank contra trickster con la semilla 0 para `clutch_ko`, porque es la misma pelea que el vector de knockdown con los lados intercambiados. El empate existe, así que RF9 queda cubierto por el dorado y por el test unitario.
12. **Hallazgos bajos de QA, corregidos antes del PR:**
    - (Superado por la desviación 13.) `checkWasmSources` contaba cualquier atributo `allow`, `expect` o `warn` cuya lista mencione `unsafe_code` (también `#[allow(dead_code, unsafe_code)]`), falla si alguno es de crate (`#![…]`) y falla si el único que hay no está justo encima de la `pub fn` con `#[wasm_bindgen]` (se admiten comentarios y otros atributos en medio). Cada caso tiene su test negativo en Vitest.
    - `_check-wasm` pasa `-- --nocapture` a los tests de WASI: con `panic = abort` un fallo es un trap, y sin eso solo se veía `trap: unreachable`. Ahora sale el mensaje del `panic!` (lo comprobé con un dorado corrompido).
13. **`engine-wasm` en `forbid` (revisión del PR #39, enmienda de RF19 y de la preocupación 4 del 2026-10-27).** Reemplaza las filas de `crates/engine-wasm/Cargo.toml` y `src/lib.rs` de "Archivos que cambian", la parte de RF19 de "Pruebas" y la desviación 12 en lo que toca a `checkWasmSources`:
    - `engine-wasm` usa `[lints] workspace = true` (hereda `unsafe_code = "forbid"`) y no lleva ningún `allow(unsafe_code)`. Compila para `wasm32-unknown-unknown`, y con un `allow` de prueba el compilador da `E0453`.
    - `checkWasmSources` quita los comentarios (`//` y `/* */` anidados, sin confundir `//` dentro de cadenas) y falla si `unsafe_code` aparece fuera de una lista `deny(…)`/`forbid(…)`. Así cubre `allow`, `expect` y `warn` directos o de crate, dentro de `cfg_attr` y en macros (`macro_rules!` que genera `#[allow($l)]` o la invocación que pasa `unsafe_code`). También falla con `include!` y con `#[path` (incluido `cfg_attr(…, path = …)`), y conserva la búsqueda del token `unsafe` sobre el texto completo.
    - `checkWasmLints` exige `[lints] workspace = true`, sin otras claves en `[lints]`, sin tablas `[lints.*]` y sin claves `lints.…` con puntos, y que el workspace tenga `unsafe_code = "forbid"`.
    - Hay un test negativo de Vitest por variante (las del script `bypass.mjs` de security incluidas) y casos de comentarios sin falsos positivos ni negativos.
    - Nits: `allow-invalid = true` en todas las rutas de `rand_core` de `crates/engine/clippy.toml`; el comentario de `apps/server/Dockerfile` menciona los targets wasm que ahora baja rustup (sin cambio de comportamiento); el filtro `images` de `ci.yml` incluye `Cargo.toml`.
14. **`CLAUDE.md`:** lo editó el orquestador en el commit `0bb7ec8` (targets wasm y `_check-engine`/`_check-wasm` en `check-rust`). Queda pendiente del OK del owner al fusionar.

## Revisión

### architect — 2026-10-26 — APPROVE
- Verifiqué:
  - **Frontmatter:** válido según `docs/process.md`, con `reviewers: [architect]` y el autor distinto del revisor.
  - **Cobertura de la fase A:** RF1 a RF11, RF16, RF18 a RF21, la parte de RF22 que toca a 003 y los RNF de `overflow-checks` y dependencias. Cada uno tiene archivo y prueba concreta. RF12 a RF15 y RF17 quedan explícitamente en la fase B.
  - **ADR-0001:** motor puro y entero, sin mapas, floats, reloj ni logging, con lints, `clippy.toml` y `check-engine.mjs`. Todas las constantes de juego, rangos de validación incluidos, viven en `Ruleset` con campos privados. `-> Result` es el refinamiento ya aceptado en la preocupación 1.
  - **ADR-0002:** `Seed` sin `Clone`, `Serialize`, `Display` ni getter, y con `Debug` redactado. Hay un test de que ningún dorado contiene la semilla, y el PRNG solo se consume por `roll` en el orden de RF7. El jitter se consume siempre (decisión 9), lo que es literal de RF7.
  - **ADR-0007:** el log solo lleva códigos, y los de `engine-wasm` siguen el patrón `^[a-z_]+\.[a-z_]+$`.
  - **Cotas:** recalculé `A = ⌊(84 900 − 3400) / 1200⌋ + 1 = 68` y `1 + 2·68·3 + 10 = 418 < 512`. La guarda de `max_events` (len + 3 + 9 antes de cada acción) es conservadora y no se dispara en v1. También comprobé que la recuperación y los windups dan `t` monótono y ≥ 600 ms entre resoluciones.
  - **Orden y fórmulas:** el orden del daño (`d_nc` con desesperación y debilidad, sin el ×3/2), el knockdown, la evaluación de `comeback` con las marcas previas, el turno y el tope coinciden con la tabla de la spec.
  - **Toolchain y CI:**
    - `rust-toolchain.toml` y su fixture de `check-compose`, y los targets de `deny.toml` con su test.
    - `Cargo.toml` (miembros, `rand_chacha` sin features por defecto, `overflow-checks`, `opt-level` de dev solo en dependencias, que no altera resultados enteros).
    - `_check-wasm`: comprueba los targets, corre clippy en `wasm32-unknown-unknown` y el runner WASI vía `CARGO_TARGET_WASM32_WASIP1_RUNNER`, que cargo parte por espacios. La ruta del repo y de este worktree no tiene espacios.
    - El orden de `check-rust` deja los chequeos sin Docker antes de `db-up`.
    - El filtro `rust` de `ci.yml` suma los dos scripts nuevos, y el filtro `web` ya cubre `scripts/**` y `crates/engine*/**`.
    - El `Dockerfile` del server copia `crates/` entero, así que los miembros nuevos no rompen `cargo chef`. La descarga extra de los targets wasm la acepta la preocupación 6.
    - El test nuevo de `WORKSPACE_MEMBERS` justifica que `content` y `sim` entren en la fase B.
  - **No bloqueantes de la `## Revisión` de la spec:**
    - security-reviewer: `max_events` (decisión 1), cota analítica (decisión 2), RF21 (decisión 3), y rutas completas con `allow-invalid` más su verificación manual (decisión 4 y Riesgos). Lo de TA-13 queda fuera de 003.
    - game-designer: `w_special_desperation_mul` (decisión 5). Lo de "ningún `kind` muerto" es de la fase B.
    - product-owner: la puerta "sin `timeline_version` 2 no se publican peleas reales" va a `docs/architecture.md`. La ruta `time_up` queda cubierta por `log-decision-a/b.json`, y el pedido de v2 en Linear por el paso 11.
- **Enmienda editorial de la spec, aplicada por mí** (autor de la spec, registrada en su `## Revisión`):
  - RF21 con la cota de 418 y `SpecError::EventLimit`;
  - `event_limit` en RF4, con "no falla" acotado a los rulesets que pasan el test de cotas;
  - RF2 aclara que `hp = 0` puede venir de un `block` (lo implicaba RF9, y el plan lo trata así);
  - `FightLog::outcome()` devuelve `Option` (decisión 7).
  
  El paso 9 ya no tiene que tocar la spec, solo `docs/architecture.md` y las desviaciones.
- **`CLAUDE.md`:** es razonable y correcto no editarlo en el PR. Propón en la descripción del PR el texto exacto para el owner: `check-rust` suma `_check-engine` y `_check-wasm` (los dos antes de `db-up`, sin Docker), y en Requisitos, que `rustup toolchain install` en el repo trae los targets `wasm32-unknown-unknown` y `wasm32-wasip1`. Lo aplica el owner o lo autoriza explícitamente en chat.
- No bloqueante (para la implementación o el PR):
  1. Escribir en el plan o en el PR el texto exacto de la extensión de `_check-ts-rs-graph` a `engine` y `engine-wasm` (`^(ts-rs|tokio) ` en `cargo tree -p engine -e normal`), igual que las otras recetas.
  2. Fijar en `golden.rs`, con un comentario, qué pares de fixtures y qué orientación usa el digest de 1000 peleas (por ejemplo, el par `i mod 16`), porque cambiarlo después regenera `digest-1000.txt`.
  3. Los `deny` de `lib.rs` (`indexing_slicing`, `panic`) también alcanzan a los `#[cfg(test)]` de `src/`. Si hace falta, usar un `#[allow]` local en el módulo de tests y nunca en el código del motor. Comprobar también que `allow-unwrap-in-tests` cubre los helpers de `tests/common/mod.rs`. Si no los cubre, que devuelvan `Result` o usen `assert!`.
  4. El reporte a 002 del KO por chip de bloqueo va junto con el pedido de la v2 (paso 11).
- Gate humano: el plan es `risk: high`, así que según `docs/process.md` falta el OK explícito del owner en chat antes de empezar la implementación. Esta aprobación no lo reemplaza.
