---
id: "003"
title: Motor de pelea v0
type: spec
status: in-review
risk: high
author: architect
reviewers: [product-owner, security-reviewer, game-designer]
approvals: []
linear: TA-3
depends_on: ["001"]
created: 2026-10-26
updated: 2026-10-26
---

# Spec: Motor de pelea v0

> Intent: [intent.md](./intent.md)

## Resumen
`crates/engine` expone `simulate(seed, a, b, ruleset)`, una función pura, entera y determinista que produce un `FightLog`, cuyo formato vive en `crates/protocol` y se exporta a TS. Además, un cargador `crates/content` lee y valida `content/characters/<slug>/character.json` fuera del motor, `crates/sim` corre simulaciones masivas y mide el balance, y `crates/engine-wasm` deja el motor listo para el navegador. Un test dorado demuestra que nativo y WASM producen los mismos bytes. Esta feature es **dueña del formato del log**, y 002 (renderer) y el narrador (ADR-0010) lo consumen.

Se entrega en dos fases, con un PR y un plan cada una:
- **Fase A** (`plan-a.md`): tipos de `protocol`, `engine`, `engine-wasm`, los tests dorados y el chequeo WASM. Sale primero para que 002 consuma los tipos reales.
- **Fase B** (`plan-b.md`): `content`, los personajes de prueba, `sim` y el test de balance. Ajustar las constantes del ruleset en esta fase regenera los dorados, y eso está permitido mientras no exista una pelea de producción (RF10).

## Requisitos funcionales

### Formato del log (`crates/protocol`)
- **RF1.** `protocol` define `FightLog`, `FighterInfo`, `FightEvent`, `Side`, `AbilityKind`, `MomentKind` y `EndReason` exactamente como en [Modelo de datos](#modelo-de-datos--tipos-protocol), con la derivación `ts` y exportados por `just gen`. Ningún tipo usa enteros de 64 bits, `HashMap` ni `f32`/`f64`. Cada variante tiene un test de forma JSON exacta, como los de `ws.rs`.
- **RF2.** Invariantes de todo `FightLog` que produce el motor, comprobados por test en cada simulación de los tests de propiedad:
  - el primer evento es `intro` con `t = 0`;
  - `t` nunca decrece;
  - hay exactamente un `end`, y es el último evento;
  - `hp` nunca sube y nunca supera el `max_hp` de su lado;
  - si `reason = ko`, hay exactamente un `ko` y está justo antes de `end`; si es `decision` o `draw`, hay exactamente un `time_up` y no hay ningún `ko`;
  - `winner` es `null` si y solo si `reason = draw`;
  - hay como máximo 512 eventos;
  - `end.t ≤ time_limit_ms + end_ms`.
- **RF3.** El log no contiene texto para humanos. `slug` y `ability` son códigos validados (RF12), que la UI traduce desde el lore de cada locale (ADR-0007). El log **nunca** contiene la semilla ni nada derivado de ella que no sea el propio desarrollo de la pelea (RF20).

### Motor (`crates/engine`)
- **RF4.** API pública:
  - `simulate(seed: &Seed, a: &CharacterSpec, b: &CharacterSpec, ruleset: &Ruleset) -> Result<FightLog, SpecError>`. Valida los dos personajes contra el ruleset y después simula. La simulación en sí no falla.
  - `Seed` es un newtype de `[u8; 32]`, sin `Serialize`, con `Debug` que imprime `Seed(<redacted>)`.
  - `Ruleset` solo se construye con `Ruleset::for_version(u16) -> Option<Ruleset>` (en v0, solo existe `1`). Sus campos son de solo lectura.
  - `SpecError` es un enum de códigos sin texto, por ejemplo `stat_out_of_range` o `budget_exceeded`.
  - `canonical_json(&FightLog) -> Vec<u8>`.
- **RF5.** El motor es puro:
  - sin I/O, sin reloj, sin hilos y sin logging (ni `tracing` ni `log`);
  - sin `HashMap`/`HashSet`, sin `f32`/`f64`, sin `rand` y sin `getrandom`;
  - toda la aritmética del resultado es entera (`u32`/`u64` internos), con el orden de operaciones de la tabla de fórmulas.
  
  Se hace cumplir con `#![deny(clippy::float_arithmetic)]`, un `crates/engine/clippy.toml` (`disallowed-types`: `HashMap`, `HashSet`, `Instant`, `SystemTime`; repite las dos opciones del `clippy.toml` raíz porque no se combinan) y una receta `_check-engine-graph`. Esa receta falla si el grafo normal de `engine` contiene `getrandom`, `rand`, `tokio`, `ts-rs` o `tracing`.
- **RF6.** PRNG: `rand_chacha::ChaCha20Rng::from_seed(seed)` (`=0.9.0`, `default-features = false`, la versión que ya está en `Cargo.lock`). Solo se consume con `next_u32()`, a través de una única función `roll(n)`: muestreo por rechazo (se descarta `x ≥ n·⌊2³²/n⌋` y se devuelve `x mod n`). No se usan las distribuciones de `rand`, porque su algoritmo puede cambiar entre versiones.
- **RF7.** Orden de consumo del PRNG, fijado por `ruleset_version`. Al empezar, `roll(2)` decide la iniciativa. Después, en cada acción y en este orden:
  1. `roll(peso_total)`: elección de la acción.
  2. `roll(1000)`: defensa (esquiva, bloqueo o golpe).
  3. Solo si es golpe o bloqueo, `roll(31)`: varianza.
  4. Solo si es golpe, `roll(1000)`: crítico.
  5. `roll(201)`: jitter del siguiente turno del atacante.

  No se consume nada más.
- **RF8.** Simulación por **turnos con tiempo simulado** (en ms enteros), según el [modelo de simulación](#modelo-de-simulación-ruleset-v1). Actúa el luchador con menor `next_at`. En empate actúa el que no actuó último, y si ninguno ha actuado, el que ganó la iniciativa. No se empieza un ataque si `next_at + windup_special > time_limit_ms`. Cuando ninguno puede atacar, se emiten `time_up` en `time_limit_ms` y `end`.
- **RF9.** Resultado. KO cuando el `hp` llega a 0: gana el atacante. Si se acaba el tiempo, gana quien tenga mayor `hp·1000/max_hp` (división entera), con `reason = decision`; si quedan iguales, `winner = null` y `reason = draw`.
- **RF10.** Versionado:
  - Cualquier cambio en las fórmulas, constantes, el orden del PRNG, la validación o la serialización cambia los bytes del log, y eso se detecta porque fallan los dorados (RF16).
  - **Antes** de la primera pelea guardada en producción, `ruleset_version = 1` puede cambiar si se regeneran los dorados en el mismo PR.
  - **Después**, todo cambio crea una versión nueva y la anterior se conserva en el código, para que `for_version(n)` siga re-simulando las peleas viejas (ADR-0001).
  - Un bump de `rand_chacha`, `serde` o `serde_json` se acepta solo si los dorados pasan sin regenerarse.
- **RF11.** Serialización canónica: `serde_json::to_vec(&log)` compacto, con los campos en el orden de declaración, sin mapas ni floats. "Idéntico byte a byte" significa que `canonical_json` devuelve los mismos bytes.

### Personajes y cargador (`content/`, `crates/content`)
- **RF12.** Esquema de `content/characters/<slug>/character.json` en [Esquema de personaje](#esquema-de-personaje). `crates/content` lo parsea con `deny_unknown_fields`, valida rangos, presupuesto, unicidad y expresiones regulares, comprueba que `slug` coincide con el nombre del directorio y que existen `lore.es.md` y `lore.en.md` (solo que existan; el lore no se parsea en 003), y entrega `engine::CharacterSpec`. `load_roster(root)` devuelve los personajes **ordenados por `slug`**, porque `read_dir` no garantiza el orden. Las reglas de validación viven en el motor (`CharacterSpec::validate`), y `content` solo hace I/O y conversión, de modo que el WASM valida lo mismo.
- **RF13.** Un test de `crates/content` carga el `content/characters/` real y falla si algún personaje no es válido.
- **RF14.** Fase B: de 2 a 4 personajes homenaje originales con `character.json`, `lore.es.md` y `lore.en.md` y el checklist de ADR-0004. Los diseña el game-designer en la implementación. Se actualizan el skill `write-character` (pasa de YAML con pesos 0–1 a JSON con enteros 0–100) y `content/characters/README.md`.

### Simulador (`crates/sim`)
- **RF15.** `crates/sim` es una lib más un bin (`cargo run -p sim --release -- …` y la receta `just sim *args`). Flags:
  - `--fights N` (por defecto 10000);
  - `--seed <64 hex>` (por defecto, una constante fija);
  - `--content <dir>` (por defecto `content/characters`);
  - `--json`;
  - `--check`: código de salida 1 si falla algún criterio de RF17.

  Esquema:
  - las semillas de cada pelea salen, de 32 en 32 bytes, de un `ChaCha20Rng` sembrado con `--seed`, así que el informe es reproducible;
  - para cada par no ordenado de personajes `{i, j}` se usan `m = ⌊N / (2·P)⌋` semillas, y **cada semilla se juega en las dos orientaciones** (`i` como `a` y como `b`);
  - además, cada personaje juega contra sí mismo con `m` semillas, para medir el sesgo del lado `a`.

  Informe (texto en inglés, porque es una herramienta de desarrollo y no UI):
  - win rate global por personaje, `(victorias + empates/2) / apariciones`, en ‰;
  - la matriz de enfrentamientos;
  - % de victorias del lado `a` en los espejos;
  - % de KO, decisión y empate;
  - duración (fin de `end.t`) en p10, p50 y p90;
  - % de peleas con al menos un `moment`, y el desglose por `kind`;
  - tiempo total (simulación más `canonical_json` de cada log), medido con `Instant` en el bin, nunca en el motor.

  `print_stdout` se permite solo con un `#[allow]` local en `main.rs`. El CLI nunca recibe semillas de peleas reales.

### Pruebas de determinismo y WASM
- **RF16.** Dorados en `crates/engine/tests/golden.rs`. Los fixtures de personajes viven en `crates/engine/tests/fixtures/`, desacoplados de `content/`, para que balancear no rompa los dorados. Incluyen:
  - (a) al menos 6 vectores `(seed, a, b) → log`, comparados contra `tests/golden/*.json` con `include_bytes!`. Cubren `ko` y `decision`, y también `draw` si el plan encuentra una semilla que lo produzca con fixtures defensivos; si no, `draw` se cubre con un test unitario de la regla de RF9;
  - (b) un digest FNV-1a de 64 bits (implementado en el test, sin crate) de la concatenación de `canonical_json` de 1000 peleas con semillas derivadas, comparado con un valor commiteado.

  Los dorados se regeneran con `UPDATE_GOLDEN=1`, solo en nativo.
- **RF17.** Criterios de balance. Los comprueba un test de `crates/sim` sobre el `content/` real, con 10 000 peleas y la semilla por defecto, dentro de `just check`. Es determinista, así que no puede ser *flaky*:
  - el win rate global de cada personaje está entre 400 ‰ y 600 ‰;
  - al menos el 15 % de las peleas tienen algún `moment`;
  - al menos el 80 % de las peleas terminan con `end.t` entre 45 000 y 92 000 ms;
  - `time_up` en como mucho el 20 % de las peleas, y `draw` en menos del 2 %;
  - en los espejos, el lado `a` gana entre el 47 % y el 53 %.
  
  Los umbrales de duración, `time_up` y sesgo son valores iniciales de esta spec, y el game-designer puede ajustarlos en su revisión.
- **RF18.** WASM:
  - `rust-toolchain.toml` agrega los targets `wasm32-unknown-unknown` y `wasm32-wasip1`;
  - `check-rust` corre `cargo clippy -p engine -p engine-wasm --target wasm32-unknown-unknown --locked -- -D warnings`, que demuestra que el motor y los bindings compilan para el navegador;
  - `check-rust` corre también `cargo test -p engine --test golden --target wasm32-wasip1 --locked`, con `CARGO_TARGET_WASM32_WASIP1_RUNNER` apuntando a `scripts/wasi-runner.mjs` (un runner de unas 30 líneas sobre `node:wasi` de Node, sin wasmtime ni crates nuevos). Así, los **mismos** dorados de RF16 pasan en nativo y en WASM, y la igualdad byte a byte queda demostrada contra los mismos archivos.
  
  El runner convierte un trap (`panic = abort`) en el código de salida 101.
- **RF19.** `crates/engine-wasm` (mínimo) es un `cdylib` con `wasm-bindgen` (`=0.2.129`, ya en el lock). Exporta una sola función: `simulate(seed_hex: &str, a_json: &str, b_json: &str, ruleset_version: u16) -> Result<String, JsError>`, que devuelve `canonical_json` como `String`. Los errores son códigos (`spec.*`, `seed.invalid_hex`, `ruleset.unknown`), sin texto. En 003 no hay glue JS ni la usa la web: la ejecución en navegador es parte de la feature de verificación de repeticiones.

### Seguridad
- **RF20.** El motor:
  - no registra ni serializa la semilla: `Seed` no tiene `Serialize` y su `Debug` está redactado;
  - no la incluye en `FightLog`;
  - no tiene dependencias de logging.

  Lo comprueban un test (el `Debug` no contiene ningún byte en hex) y `_check-engine-graph`.
- **RF21.** `simulate` es total con entradas no confiables, porque `engine-wasm` recibe JSON del cliente. Cumple esto:
  - la validación rechaza lo que esté fuera de rango;
  - el bucle termina, porque cada acción avanza el reloj por lo menos `interval_min + windup_strike` = 1400 ms;
  - el log está acotado por RF2;
  - no hay `unwrap`, `expect` ni índices que puedan entrar en pánico (los lints del workspace y la revisión).
  
  Un test alimenta `simulate` con specs inválidas y extremos válidos (todos los stats en 1, en 100 o en el presupuesto exacto) y espera `Err` o un log que cumpla RF2.

## Requisitos no funcionales
- **Rendimiento:** 10 000 simulaciones, con `canonical_json` incluido, en menos de 10 s con `--release` en la Mac del owner, según el tiempo que reporta `sim`. CI no lo asegura, porque el runner es ruidoso. En producción, el motor corre una vez por pelea al cerrar las apuestas: microsegundos y KB en el VPS de 1 vCPU / 4 GB.
- **`overflow-checks = true`** en `[profile.release.package.engine]`: un desbordamiento entra en pánico en lugar de divergir en silencio entre perfiles. Las fórmulas están acotadas, y los `u32`/`u64` internos no se desbordan con stats ≤ 100.
- **i18n:** el log solo lleva códigos. Los nombres de los personajes y las habilidades viven en `lore.<locale>.md`, y su parseo y su validación en CI corresponden a la feature que los muestre.
- **Dependencias:** no se agrega ninguna versión nueva al lock. `rand_chacha 0.9.0` (con `rand_core` y `ppv-lite86`), `wasm-bindgen 0.2.129`, `serde` y `serde_json` ya están en él. Aun así, se corre `just crate-age` y se agregan `engine`, `engine-wasm`, `content` y `sim` a `WORKSPACE_MEMBERS` de `scripts/check-crate-age.mjs` y a `[workspace].members`.

## Diseño

### Componentes afectados
| Pieza | Cambio |
|---|---|
| `crates/protocol` | Nuevo `src/fight.rs` con los tipos del log y sus tests de forma. `just gen` agrega los `.ts` a `packages/protocol-ts`. |
| `crates/engine` (nuevo) | `simulate`, `Seed`, `Ruleset`, `CharacterSpec`, `SpecError` y `canonical_json`. Depende de `protocol` (sin `ts`), `serde`, `serde_json` y `rand_chacha`. |
| `crates/engine-wasm` (nuevo) | `cdylib` con `wasm-bindgen`. Depende de `engine` y `serde_json`. |
| `crates/content` (nuevo) | Lee y valida `content/characters/`. Depende de `engine`, `serde`, `serde_json` y `thiserror`. Lo usarán `sim` y, más adelante, el server. |
| `crates/sim` (nuevo) | Lib y bin del simulador. Depende de `engine`, `content` y `serde_json`. Va en `crates/` y no en `tools/` porque el filtro `rust` de CI cubre `crates/**`. |
| `content/characters/*` | De 2 a 4 personajes (fase B). |
| `rust-toolchain.toml`, `Cargo.toml`, `justfile`, `scripts/wasi-runner.mjs`, `scripts/check-crate-age.mjs` | Targets, miembros y recetas (`sim`, `_check-wasm`, `_check-engine-graph`, conectadas a `check-rust`). |
| Docs | Al cerrar: `docs/architecture.md` (firma `-> Result`, nuevos crates, serialización canónica), el skill `write-character` y `content/characters/README.md`. |

Dirección de dependencias: `protocol ← engine ← {content, engine-wasm, sim}` (y el server, en el futuro). El motor depende de `protocol` sin la feature `ts`, y `_check-ts-rs-graph` se extiende a `engine`. No se elige "el motor define y `protocol` reexporta" porque obligaría a `protocol` a depender del motor y a meter `ts-rs` en el motor para exportar los tipos.

### Modelo de datos / tipos (protocol)
```rust
// crates/protocol/src/fight.rs — every type: Debug, Clone, PartialEq, Eq, Serialize, Deserialize,
// #[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]

#[serde(rename_all = "snake_case")]
pub enum Side { A, B }                                   // "a" | "b"

#[serde(rename_all = "snake_case")]
pub enum AbilityKind { Strike, Heavy, Special }          // renderer: attack_light / attack_heavy / special

#[serde(rename_all = "snake_case")]
pub enum MomentKind { Comeback, DecisiveCrit, SuddenKo }

#[serde(rename_all = "snake_case")]
pub enum EndReason { Ko, Decision, Draw }

pub struct FighterInfo {
    pub slug: String,      // character code, e.g. "rata-electrica"
    pub max_hp: u16,
}

pub struct FightLog {
    pub ruleset_version: u16,
    pub a: FighterInfo,
    pub b: FighterInfo,
    pub events: Vec<FightEvent>,
}

/// `t` = ms since the start of the fight (playback offset). The target of an attack is
/// always the side opposite to `attacker`.
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FightEvent {
    Intro   { t: u32 },
    Attack  { t: u32, attacker: Side, ability: String, kind: AbilityKind },
    Hit     { t: u32, attacker: Side, ability: String, damage: u16, crit: bool, weak: bool, hp: u16 },
    Block   { t: u32, attacker: Side, ability: String, damage: u16, hp: u16 },
    Dodge   { t: u32, attacker: Side, ability: String },
    LowHp   { t: u32, side: Side, hp: u16 },
    Moment  { t: u32, kind: MomentKind, side: Side },
    Ko      { t: u32, side: Side },                      // side = the one knocked out
    TimeUp  { t: u32 },
    End     { t: u32, winner: Option<Side>, reason: EndReason },
}
```
Más algunos detalles:
- `ability` es `"strike"` (el golpe básico, que no aparece en el contenido) o el `id` de una habilidad del personaje.
- `damage` es el daño calculado, que puede superar el `hp` restante y está topado en 9999. `hp` es el que le queda al objetivo después del golpe.
- `FightLog::outcome() -> (Option<Side>, EndReason)` lee el `end`; no hay un campo duplicado.
- No se guarda `duration_ms`: la duración es `end.t`.

Ejemplo recortado e ilustrativo. Entre 6800 y 64 200 ms se omitieron eventos, así que los tiempos y los `hp` no cuadran con las fórmulas; una pelea real tiene entre 100 y 300 eventos.
```json
{"ruleset_version":1,"a":{"slug":"rata-electrica","max_hp":600},"b":{"slug":"guerrero-erizado","max_hp":660},"events":[
{"type":"intro","t":0},
{"type":"attack","t":3600,"attacker":"a","ability":"strike","kind":"strike"},
{"type":"hit","t":4000,"attacker":"a","ability":"strike","damage":22,"crit":false,"weak":false,"hp":638},
{"type":"attack","t":4300,"attacker":"b","ability":"spirit_wave","kind":"special"},
{"type":"dodge","t":5400,"attacker":"b","ability":"spirit_wave"},
{"type":"attack","t":5700,"attacker":"a","ability":"thunder_squeak","kind":"special"},
{"type":"hit","t":6800,"attacker":"a","ability":"thunder_squeak","damage":96,"crit":true,"weak":false,"hp":542},
{"type":"hit","t":40200,"attacker":"b","ability":"rising_fist","damage":61,"crit":false,"weak":false,"hp":140},
{"type":"low_hp","t":40200,"side":"a","hp":140},
{"type":"block","t":51900,"attacker":"b","ability":"rising_fist","damage":14,"hp":126},
{"type":"moment","t":58300,"kind":"comeback","side":"a"},
{"type":"attack","t":63100,"attacker":"a","ability":"thunder_squeak","kind":"special"},
{"type":"hit","t":64200,"attacker":"a","ability":"thunder_squeak","damage":118,"crit":true,"weak":false,"hp":0},
{"type":"moment","t":64200,"kind":"decisive_crit","side":"a"},
{"type":"ko","t":64200,"side":"b"},
{"type":"end","t":66200,"winner":"a","reason":"ko"}]}
```
(`canonical_json` no tiene saltos de línea; aquí se agregaron para leerlo.)

**Orden dentro de un mismo `t`**, al resolver un ataque:
1. `hit`, `block` o `dodge`;
2. `low_hp`;
3. `moment` `comeback`;
4. y, si hay KO: `moment` `decisive_crit`, `moment` `sudden_ko` y `ko`.

El `end` va en `t + end_ms`.

Para el narrador (ADR-0010), los triggers se derivan del evento:
- inicio: `intro`;
- crítico: `hit` con `crit`;
- esquiva: `dodge`;
- HP bajo: `low_hp`;
- remontada: `moment` `comeback`;
- KO: `ko`.

El "favorito perdiendo" depende de los pozos, así que lo calcula el server o el narrador, no el motor.

### Modelo de simulación (ruleset v1)
Los valores son iniciales: se afinan en la fase B con `sim` y quedan en `Ruleset` (código, no contenido). En la tabla, "‰" es sobre 1000 y todas las divisiones son enteras, en el orden escrito.

| Concepto | Regla v1 |
|---|---|
| Presupuesto de stats | `hp + attack + defense + speed + special ≤ 300`, cada uno en 1–100 |
| HP máximo | `max_hp = 300 + 6·hp` |
| Intervalo entre acciones | `interval = 1800 − 8·speed` ms, más un jitter de `roll(201)` |
| Primer turno | `next_at = intro_ms (3000) + interval/2` |
| Windup (de `attack` a la resolución) | strike 400, heavy 700, special 1100 ms |
| Próximo turno del atacante | `t_resolución + interval + jitter` |
| Aturdimiento del defensor | `next_at_def = max(next_at_def, t_resolución + 300)` |
| Elección de la acción (personalidad como pesos) | `strike`: `100 + aggression`. Cada `heavy` disponible: `50 + aggression`. Cada `special` disponible: `30 + 2·showmanship`, ×2 si el propio hp es ≤ 250 ‰. Se recorren en orden fijo (primero `strike` y después las habilidades en el orden del archivo) y se elige el primero cuyo peso acumulado supere `roll(total)`. |
| Enfriamiento | Después de usar una habilidad, queda bloqueada durante sus siguientes `cooldown` acciones propias |
| Defensa (`r = roll(1000)`) | `dodge‰ = 40 + 3·speed_def/2`. `block‰ = 60 + defense_def + caution_def`. Si `r < dodge`, esquiva; si `r < dodge + block`, bloqueo; si no, golpe. |
| Daño | `off` = `special` para las habilidades `special` y `attack` para el resto. La potencia de `strike` es 20. `base = power·(100 + off)/(100 + defense_def)`. Después: `d = base·(85 + roll(31))/100`; si es crítico, `d = d·3/2`; si es debilidad, `d = d·5/4`; si es bloqueo, `d = d·3/10`, sin crítico. En un golpe, `d = max(d, 1)`. |
| Crítico (solo en golpe) | `roll(1000) < 40 + special_atk` |
| Debilidad | La habilidad tiene `element` y ese elemento está en las `weaknesses` del objetivo: `weak = true`, ×5/4 |
| Tiempo límite | `time_limit_ms = 90 000`; `end_ms = 2000` |
| HP bajo | Primera vez que `hp·1000/max_hp ≤ 250` con `hp > 0`, una vez por lado |

**Momentos.** Se emiten como `moment`, cada `kind` como mucho una vez por lado y por pelea:
- **`comeback`**: un lado que en algún momento estuvo por debajo del rival en ≥ 350 ‰ de HP relativo (`hp·1000/max_hp`) pasa a estar estrictamente por encima. Se emite al resolver el ataque que lo provoca, aunque ese lado no termine ganando.
- **`decisive_crit`**: un crítico produce el KO y el mismo golpe sin multiplicador de crítico no lo habría producido (`d_sin_crit < hp_antes ≤ d`).
- **`sudden_ko`** ("sorpresa"): el lado noqueado tenía ≥ 500 ‰ de HP en algún momento de los 8000 ms previos al KO. El `side` del `moment` es el ganador.

### Esquema de personaje
`content/characters/<slug>/character.json`. El archivo es el de abajo, y `engine::CharacterSpec` es ese mismo objeto sin `archetype`, `ruleset_version` ni `rivals`. Esa forma motor es la que recibe el WASM.
```json
{
  "slug": "rata-electrica",
  "archetype": "electric-rodent",
  "ruleset_version": 1,
  "stats": { "hp": 50, "attack": 55, "defense": 40, "speed": 85, "special": 70 },
  "personality": { "aggression": 70, "caution": 20, "showmanship": 90 },
  "abilities": [
    { "id": "thunder_squeak", "kind": "special", "power": 70, "cooldown": 3, "element": "electric" }
  ],
  "weaknesses": ["ground"],
  "rivals": []
}
```
| Campo | Regla |
|---|---|
| `slug` | `^[a-z][a-z0-9-]{1,31}$`, igual al nombre del directorio y estable |
| `archetype` | Mismo patrón; código para el lore, que el motor ignora |
| `ruleset_version` | Igual a la versión con que se carga |
| `stats.*` | `u8` en 1–100, con la suma ≤ 300 |
| `personality.*` | `u8` en 0–100 (enteros: el 0–1 del skill actual desaparece) |
| `abilities` | De 1 a 4 |
| `abilities[].id` | `^[a-z][a-z0-9_]{1,31}$`, único en el personaje y distinto de `strike` |
| `abilities[].kind` | `heavy` (`power` 20–60, `cooldown` 0–5) o `special` (`power` 40–100, `cooldown` 2–8) |
| `abilities[].element` | Opcional, de la lista cerrada `fire`, `water`, `electric`, `ground`, `air` e `ice` |
| `weaknesses` | De 0 a 2 elementos únicos de esa misma lista |
| `rivals` | Slugs existentes en el roster, para el narrador; el motor los ignora |

Se eligió JSON con `serde_json`, que ya está en el workspace, para no agregar dependencias. YAML exigiría un crate nuevo (`serde_yaml` está archivado), y TOML sumaría cinco crates. No hay floats, así que el parseo es exacto.

### API / mensajes WS
No hay. El envío progresivo de eventos y del `end` lo define la spec del server y del scheduler (fuera de alcance).

### Flujo
```
content/characters/*/character.json ──content::load_roster──▶ Vec<CharacterSpec> (sorted by slug)
                                                                    │
Seed (32 B, server or sim) ──▶ engine::simulate(seed, a, b, Ruleset::for_version(1)) ──▶ FightLog
                                                                    │
                                     canonical_json ──▶ bytes (DB, golden, WASM, verification)
```

## Áreas de preocupación
1. **Firma de ADR-0001.** El ADR escribe `simulate(...) -> FightLog`, y aquí devuelve `Result<FightLog, SpecError>` para que `engine-wasm` pueda recibir entradas no confiables sin pánicos (RF21). Es un refinamiento, no una contradicción: la simulación de entradas válidas sigue siendo infalible. Se refleja en `docs/architecture.md` y no hace falta otro ADR.
2. **Commit-reveal incompleto (ADR-0002).** `SHA-256(seed)` solo compromete la semilla. Para re-simular hacen falta además los `CharacterSpec` exactos y la `ruleset_version`, y si el operador pudiera cambiar un spec después del commit, influiría en el resultado. **Se arrastra a la spec del server/scheduler**, con una recomendación: guardar y publicar al abrir las apuestas el snapshot de los dos `CharacterSpec` (forma motor) y la versión, y valorar extender el commit a `SHA-256(seed ‖ canonical(specs) ‖ version)`, lo que sería una enmienda de ADR-0002 con el OK del owner. El contenido puede cambiar entre temporadas, así que el slug no basta.
3. **El `FightLog` completo es tan sensible como la semilla hasta `Settled`**, porque contiene el resultado. El motor no puede protegerlo. Lo hace cumplir el server (ADR-0002 punto 3), y su spec debe tratar `FightLog` y `outcome()` con la misma regla de no serialización que la semilla.
4. **`wasm-bindgen` frente a `unsafe_code = "forbid"`.** El código que genera `#[wasm_bindgen]` usa `unsafe` y `export_name`, y `forbid` no se puede relajar con un `allow` local. Si choca, `engine-wasm` declara su propia tabla `[lints]`, igual a la del workspace pero con `unsafe_code = "deny"`, y pone `#[allow(unsafe_code)]` **solo** en el item exportado, sin `unsafe` escrito a mano. Lo valida security-reviewer en el PR de la fase A. El motor conserva `forbid`.
5. **WASI como sustituto del navegador.** El test byte a byte corre en `wasm32-wasip1`, no en el artefacto `wasm32-unknown-unknown` de `wasm-bindgen`. Es el mismo backend wasm32 y el mismo `usize` de 32 bits, y el motor no toca ninguna API de plataforma, así que la equivalencia es razonable. Aun así, la feature de verificación en navegador debe agregar un test del artefacto real (con `wasm-bindgen-cli` o lo que se use). `node:wasi` está marcado como experimental en Node 22: si su API cambia, solo se rompe el runner de 30 líneas.
6. **Imagen del server.** `apps/server/Dockerfile` copia `rust-toolchain.toml` y `rustup toolchain install` va a descargar también los dos targets wasm (unas decenas de MB, solo en la etapa builder). Es aceptable. Si devops prefiere evitarlo, el plan de la fase A lo resuelve sin cambiar el canal (`check-compose.mjs` solo enlaza la versión).
7. **Coordinación con 002**, que se trabaja en paralelo. El guion falso del spike debe escribirse con estos tipos. Si al renderer le falta algo (por ejemplo, la posición o la duración de una animación), lo pide **antes de que se fusione la fase A**, porque después el formato solo cambia con un PR de esta feature. Las animaciones de 002 se corresponden con los eventos así:
   - `attack.kind` → `attack_light`, `attack_heavy` o `special`;
   - `hit` → `hit`;
   - `block` → `block`;
   - `dodge` → `dodge`;
   - `ko` → `ko`;
   - `end` con ganador → `win`.
8. **Empates.** Los produce un `time_up` con HP relativo idéntico y deberían ser menos del 2 % (RF17). Qué pasa con las apuestas en un empate (lo natural es un reembolso) lo decide la spec de apuestas.
9. **Debilidades en v0 y balance.** Con solo 2–4 personajes, una debilidad (×5/4) puede inclinar mucho un enfrentamiento concreto. El criterio de 40–60 % es global, y una matriz asimétrica es aceptable e interesante para el parimutuel. El game-designer lo confirma en su revisión.
10. **El balance dentro de `just check`.** Un cambio de contenido que desbalancee rompe el check. Es intencional (lo dice el skill `write-character`).

## Respuestas a preguntas abiertas del intent
- **P: ¿Por turnos con tiempo simulado o con ticks continuos?** → R: Turnos con tiempo simulado (RF8). Cada acción tiene `next_at` en ms enteros, el más próximo actúa y las acciones nunca se solapan. Hay menos estado y menos consumo de PRNG que con ticks, el orden es trivialmente determinista y el renderer recibe una secuencia limpia.
- **P: ¿Cómo influye la personalidad?** → R: Como pesos sobre las acciones, con una sola tirada ponderada por turno. `aggression` sube `strike` y `heavy`, `showmanship` sube `special` (el doble en apuros) y `caution` sube el bloqueo. No hay árbol de comportamiento en v0.
- **P: ¿Debilidades por tipo en v0?** → R: Sí, mínimas: seis elementos cerrados, un `element` opcional por habilidad, hasta dos debilidades y ×5/4, sin resistencias ni estados. ADR-0001 ya las incluye en el personaje, y dan material al narrador (`weak`) por poco costo. Las revisa el game-designer.
- **Duración 45–90 s, a confirmar con 002** → `intro_ms`, `time_limit_ms` y `end_ms` son parámetros del ruleset, y RF17 mide la banda. Si 002 pide otro ritmo, se cambian las constantes y se regeneran los dorados, mientras la versión siga siendo pre-producción.

## Estrategia de pruebas
- **Unit (`engine`):**
  - `roll` (rechazo y límites);
  - la validación de `CharacterSpec`, con cada código de `SpecError`;
  - cada fórmula de la tabla;
  - los momentos, con escenarios construidos;
  - la regla de `decision`/`draw`;
  - el `Debug` redactado de `Seed`.
- **Forma (`protocol`):** JSON exacto de cada variante y de `FightLog`, y rechazo de un `type` desconocido o de campos que falten.
- **Propiedad (`engine`, sin crates nuevos):** 1000 semillas derivadas por cada par de fixtures. Cada log simulado dos veces da los mismos bytes y cumple las invariantes de RF2. Más los extremos de RF21.
- **Dorados (nativo y WASI):** RF16 y RF18, dentro de `just check` y del job `rust` de CI (que ya tiene Node).
- **Integración (`content`):** fixtures inválidos (un campo desconocido, un slug distinto del directorio, un presupuesto excedido o un lore que falta) y el `content/` real (RF13).
- **Balance (`sim`):** RF17 sobre el `content/` real. El tiempo de RF-NF se mide a mano con `just sim --release` y se pega en el PR de la fase B.
- **Grafo:** `_check-engine-graph` y `_check-ts-rs-graph`, extendido a `engine`.

## Alternativas consideradas
- **Ticks de tiempo continuo:** más estado, más tiradas por segundo, más difícil de razonar y de balancear, y sin beneficio visual, porque el renderer coreografía igual.
- **Árbol de comportamiento para la personalidad:** más expresivo, pero cuesta más diseñarlo y balancearlo. Los pesos bastan para v0 y pueden evolucionar en un ruleset futuro.
- **`FightEvent` en `engine` reexportado por `protocol`:** invierte la dependencia y mete `ts-rs` en el motor. Descartado.
- **El evento como `{ t, kind: {...} }` o con `#[serde(flatten)]`:** el anidado es menos cómodo para el renderer, y `flatten` tiene rarezas en serde y en ts-rs. Repetir `t` en cada variante es explícito y genera una unión TS limpia.
- **YAML o TOML para los personajes:** dependencias nuevas, y en YAML, crates sin mantenimiento. Se eligió JSON (ver el esquema).
- **`wasm-bindgen-cli` + Node, o wasmtime, para el test WASM:** hay que instalar una herramienta de versión acoplada (o un binario aparte) en local y en CI. WASI con `node:wasi` no agrega nada al toolchain salvo el target.
- **Exportaciones crudas sin `wasm-bindgen`:** exigen `unsafe` escrito a mano, que es peor que el `allow` acotado de la preocupación 4.
- **`rand::distributions` / `gen_range`:** su algoritmo no tiene garantía de estabilidad entre versiones de `rand`. Se usa un `roll` propio sobre `next_u32`.

## Revisión
