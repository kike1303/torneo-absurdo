---
id: "003"
title: Motor de pelea v0
type: spec
status: done
risk: high
author: architect
reviewers: [product-owner, security-reviewer, game-designer]
approvals:
  - {by: product-owner, date: 2026-10-26}
  - {by: security-reviewer, date: 2026-10-26}
  - {by: game-designer, date: 2026-10-27}
  - {by: owner, date: 2026-10-26}
linear: TA-3
depends_on: ["001"]
created: 2026-10-26
updated: 2026-10-27
---

# Spec: Motor de pelea v0

> Intent: [intent.md](./intent.md)

## Resumen
`crates/engine` expone `simulate(seed, a, b, ruleset)`, una función pura, entera y determinista que produce un `FightLog`, cuyo formato vive en `crates/protocol` y se exporta a TS. Además, un cargador `crates/content` lee y valida `content/characters/<slug>/character.json` fuera del motor, `crates/sim` corre simulaciones masivas y mide el balance, y `crates/engine-wasm` deja el motor listo para el navegador. Un test dorado demuestra que nativo y WASM producen los mismos bytes. Esta feature es **dueña del formato del log**, y 002 (renderer, a través de su adaptador de la fase C) y el narrador (ADR-0010) lo consumen.

Se entrega en dos fases, con un PR y un plan cada una:
- **Fase A** (`plan-a.md`): tipos de `protocol`, `engine`, `engine-wasm`, los tests dorados y los chequeos de WASM y del grafo. Sale primero para que 002 consuma los tipos reales.
- **Fase B** (`plan-b.md`): `content`, los personajes de prueba, `sim` y el test de balance. Ajustar los **valores** de los campos de `Ruleset` en esta fase regenera los dorados, y eso está permitido mientras no exista una pelea de producción (RF10). Cambiar la **forma** de una fórmula enmienda esta spec.

## Requisitos funcionales

### Formato del log (`crates/protocol`)
- **RF1.** `protocol` define `FightLog`, `FighterInfo`, `FightEvent`, `Side`, `AbilityKind`, `MomentKind` y `EndReason` exactamente como en [Modelo de datos](#modelo-de-datos--tipos-protocol), con la derivación `ts` y exportados por `just gen`. Ningún tipo usa enteros de 64 bits, `HashMap` ni `f32`/`f64`. Cada variante tiene un test de forma JSON exacta, como los de `ws.rs`.
- **RF2.** Invariantes de todo `FightLog` que produce el motor, comprobados por test en cada simulación de los tests de propiedad:
  - **Orden y estructura:**
    - el primer evento es `intro` con `t = 0`;
    - `t` nunca decrece;
    - hay exactamente un `end` y es el último; `end.t` es la duración total de la reproducción (intro y pose de victoria incluidas) y cumple `end.t ≤ time_limit_ms + end_ms`;
    - hay como máximo 512 eventos.
  - **HP:**
    - el `hp` de un evento es el `hp` anterior de ese lado menos `damage` (en el primer golpe, el anterior es `max_hp`); `damage` es el daño **aplicado**, así que nunca supera el `hp` anterior;
    - `hp = 0` solo aparece en la resolución (`hit` o `block`) que precede al `ko` de ese lado; un bloqueo cuyo daño deja `hp = 0` también es KO (RF9).
  - **Final:**
    - si `reason = ko`, hay exactamente un `ko` y `end.t ≥ ko.t + 3500`;
    - si `reason` es `decision` o `draw`, hay exactamente un `time_up` y ningún `ko`;
    - `winner` es `null` si y solo si `reason = draw`.
  - **Ritmo (datos de 002):**
    - la primera resolución (`hit`, `block` o `dodge`) llega a `t ≥ 2000`;
    - dos resoluciones consecutivas están separadas por ≥ 600 ms;
    - cada `knockdown` tiene el mismo `t` que un `hit` sobre ese lado y un `duration_ms` de 800 a 3000;
    - ninguna resolución cae en `(knockdown.t, knockdown.t + duration_ms]`.
- **RF3.** El log no contiene texto para humanos. `slug` y `ability` son códigos validados (RF12), que la UI traduce con el lore de cada locale (ADR-0007). El log **nunca** contiene la semilla ni nada derivado de ella que no sea el propio desarrollo de la pelea (RF20).

### Motor (`crates/engine`)
- **RF4.** API pública:
  - `simulate(seed: &Seed, a: &CharacterSpec, b: &CharacterSpec, ruleset: &Ruleset) -> Result<FightLog, SpecError>`. Valida los dos personajes contra el ruleset y después simula; la simulación en sí no falla con ningún ruleset que pase el test de cotas de `Ruleset` (incluida la cota de eventos de RF21).
  - `Seed` es un newtype de `[u8; 32]` que solo se construye con `Seed::from_bytes([u8; 32])`. No implementa `Serialize`, `Display` ni `LowerHex`/`UpperHex`, no expone sus bytes, y su `Debug` imprime exactamente `Seed(<redacted>)`. El server conserva su propia copia de los bytes para el commit y la base de datos.
  - `Ruleset` solo se construye con `Ruleset::for_version(u16) -> Option<Ruleset>` (en v0, solo existe `1`). Sus campos son de solo lectura y son **todas** las constantes de la [tabla del ruleset](#modelo-de-simulación-ruleset-v1); ninguna constante de juego vive fuera de `Ruleset`.

    > **Enmienda (006, TA-21).** Ver la [spec de 006](../006-ritmo-rapido/spec.md), RF5 y RF11. Dos familias de constantes viven fuera de `Ruleset` y **no** son constantes de juego, así que no violan esta regla: (1) el contrato del consumidor, `engine::TIMELINE_CONTRACT` (`crates/engine/src/contract.rs`), espejo de `TIMELINE_LIMITS` del renderer, que la simulación nunca lee y que no cambia ningún byte del log (lo usan el test de cotas, `assert_invariants` y `sim`); y (2) las definiciones de medida de `sim`, `DURATION_BAND_MS`, `BURST_GAP_MAX_MS` (900) y `PAUSE_GAP_MIN_MS` (2500), en `crates/sim/src/report.rs`. Toda entrada de la simulación, incluidos los cuatro campos `breather_*`, sigue siendo un campo de `Ruleset`.
  - `CharacterSpec` implementa `Serialize` y `Deserialize` con `deny_unknown_fields` (forma motor, ver [Esquema de personaje](#esquema-de-personaje)).
  - `SpecError` es un enum de códigos sin texto, por ejemplo `stat_out_of_range`, `budget_exceeded` o `event_limit` (este último es una guarda inalcanzable con un ruleset que pase el test de cotas: si se alcanzara `max_events`, `simulate` devuelve el error en lugar de truncar el log o entrar en pánico).
  - `canonical_json(&FightLog) -> Result<Vec<u8>, EncodeError>` y `canonical_spec_json(&CharacterSpec) -> Result<Vec<u8>, EncodeError>`. `EncodeError` es un código sin texto, y ninguna de las dos usa `unwrap_or_default()` ni descarta errores.
- **RF5.** El motor es puro:
  - sin I/O, sin reloj, sin hilos y sin logging (ni `tracing` ni `log`);
  - sin `HashMap`/`HashSet`, sin `f32`/`f64`, sin `rand` y sin `getrandom`;
  - toda la aritmética del resultado es entera (`u32`/`u64` internos), con el orden de operaciones de la tabla.

  Se hace cumplir así:
  - **Lints en `lib.rs`** de `engine` y de `engine-wasm`: `#![deny(clippy::float_arithmetic, clippy::indexing_slicing, clippy::panic, clippy::unreachable, clippy::todo, clippy::unimplemented)]`, además de los del workspace.
  - **`crates/engine/clippy.toml`**, que repite las dos opciones del `clippy.toml` raíz porque no se combinan:
    - `disallowed-types`: `HashMap`, `HashSet`, `Instant`, `SystemTime`, `rand_core::OsRng`;
    - `disallowed-methods`: `SeedableRng::{from_os_rng, try_from_os_rng, from_rng, seed_from_u64}`;
    - desde la fase B, también los tipos y las funciones de I/O, entorno, hilos y procesos de `std` (`fs`, `env`, `io::{stdin, stdout, stderr}`, `net`, `thread`, `process`). El único uso legítimo, la regeneración de dorados de `tests/golden.rs`, lleva un `#[allow]` local con su razón.
  - **`scripts/check-engine.mjs`**, invocado desde `check-rust` por la receta `_check-engine`. Sus funciones son puras sobre la salida de `cargo tree` y tienen tests negativos en Vitest. Falla si:
    - el grafo normal de `engine` (`cargo tree -p engine -e normal,features --target all`) contiene `getrandom`, `rand`, `tokio`, `ts-rs`, `tracing` o `log`;
    - `rand_chacha` o `rand_core` tienen activadas las features `serde`, `os_rng` o `std`.

  Resolución de features: el chequeo mira la resolución de `engine` solo, que es la que compilan `engine-wasm` y el target wasm. En `cargo build --workspace`, la unificación activa `rand_core/os_rng` (vía `tungstenite → rand/std`), pero el motor no puede invocarlo por `disallowed-methods`, y el `Serialize` de `ChaCha20Rng` no se activa (`rand_chacha/serde` no está en el lock).
- **RF6.** PRNG: `rand_chacha::ChaCha20Rng::from_seed(seed)` (`=0.9.0`, `default-features = false`, la versión que ya está en `Cargo.lock`). Solo se consume con `next_u32()`, a través de una única función `roll(n: NonZeroU32) -> u32`: muestreo por rechazo calculado en `u64`, con `zona = (2³² / n)·n`, que descarta `x ≥ zona` y devuelve `x mod n`. No se usan las distribuciones de `rand`, porque su algoritmo puede cambiar entre versiones.
- **RF7.** Orden de consumo del PRNG, fijado por `ruleset_version`. Al empezar, `roll(2)` decide la iniciativa. Después, en cada acción y en este orden:
  1. `roll(peso_total)`: elección de la acción.
  2. `roll(1000)`: defensa (esquiva, bloqueo o golpe).
  3. Solo si es golpe o bloqueo, `roll(31)`: varianza.
  4. Solo si es golpe, `roll(1000)`: crítico.
  5. `roll(201)`: jitter del siguiente turno del atacante.
  6. Solo si la acción no terminó en KO, no produjo knockdown y `since_breather >= breather_after_resolutions` (contando ya esta resolución), `roll(1000)`: respiro si el resultado es `< breather_chance_permille`.
  7. Solo si hubo respiro en el paso 6, `roll(breather_span_ms + 1)` = `roll(801)`: duración del respiro.

  No se consume nada más: la desesperación, el knockdown, los momentos y el resultado son deterministas a partir del estado. Los pasos 6 y 7 se consumen **después** de aplicar el daño, el knockdown, `low_hp`, los momentos y el KO.

  > **Enmienda (006, TA-21).** Los pasos 6 y 7 son el respiro (RF2 y RF3 de la [spec de 006](../006-ritmo-rapido/spec.md)). Antes, la lista terminaba en el paso 5.
- **RF8.** Simulación por **turnos con tiempo simulado** (en ms enteros), según el [modelo de simulación](#modelo-de-simulación-ruleset-v1):
  - **Quién actúa:** el luchador con menor `next_at`. En empate actúa el que no actuó último, y si ninguno ha actuado, el que ganó la iniciativa.
  - **Recuperación:** después de **cualquier** resolución (golpe, bloqueo o esquiva), `next_at_def = max(next_at_def, t_res + recovery_ms)`. Un knockdown la alarga (ver la tabla).
  - **Tope:** no se empieza un ataque si `next_at + windup_special > time_limit_ms`. Cuando ninguno de los dos puede atacar, se emiten `time_up` en `t = time_limit_ms` y `end` en `time_limit_ms + end_ms`.
- **RF9.** Resultado:
  - **KO:** cuando el `hp` llega a 0, gana el atacante, y el `end` va en `ko.t + end_ms`.
  - **Por tiempo:** gana quien tenga mayor `hp·1000/max_hp` (división entera), con `reason = decision`. Si quedan iguales, `winner = null` y `reason = draw`.
- **RF10.** Versionado:
  - Cualquier cambio en las fórmulas, los valores de `Ruleset`, el orden del PRNG, la validación o la serialización cambia los bytes del log, y eso se detecta porque fallan los dorados (RF16).
  - **Antes** de la primera pelea guardada en producción, `ruleset_version = 1` puede cambiar si se regeneran los dorados en el mismo PR.
  - **Después**, todo cambio crea una versión nueva y la anterior se conserva en el código, para que `for_version(n)` siga re-simulando las peleas viejas (ADR-0001).
  - Un bump de `rand_chacha`, `serde` o `serde_json` se acepta solo si los dorados pasan sin regenerarse.
- **RF11.** Serialización canónica: `serde_json::to_vec` compacto, con los campos en el orden de declaración, sin mapas ni floats. Vale tanto para `FightLog` como para `CharacterSpec`. "Idéntico byte a byte" significa que `canonical_json` devuelve los mismos bytes.

### Personajes y cargador (`content/`, `crates/content`)
- **RF12.** Esquema de `content/characters/<slug>/character.json` en [Esquema de personaje](#esquema-de-personaje). `crates/content`:
  - lo parsea con `deny_unknown_fields`;
  - comprueba que `slug` coincide con el nombre del directorio y que existen `lore.es.md` y `lore.en.md` (solo que existan; el lore no se parsea en 003);
  - entrega `engine::CharacterSpec`.

  `load_roster(root: &Path, ruleset: &Ruleset) -> Result<Vec<Character>, ContentError>` devuelve los personajes **ordenados por `slug`**, porque `read_dir` no garantiza el orden. `Character` lleva el `CharacterSpec` validado más `archetype` y `rivals` (para el narrador y el server), y `ruleset` es la versión contra la que se valida y con la que se compara `ruleset_version`. Las reglas de validación (rangos, presupuesto, unicidad y expresiones regulares) viven en el motor (`CharacterSpec::validate`), y `content` solo hace I/O y conversión, así que el WASM valida lo mismo. Para `archetype`, que no está en la forma motor, el motor exporta `is_valid_slug(&str) -> bool`, la misma regla que usa `validate`.
- **RF13.** Un test de `crates/content` carga el `content/characters/` real y falla si algún personaje no es válido.
- **RF14.** Fase B: de 2 a 4 personajes homenaje originales con `character.json`, `lore.es.md` y `lore.en.md`, y el checklist de ADR-0004. Los diseña el game-designer en la implementación. También se actualizan:
  - el skill `write-character`: pasa de YAML con pesos 0–1 a JSON con enteros 0–100, y fija que los nombres localizados de las habilidades viven en el frontmatter de `lore.<locale>.md` como `abilities: { <id>: <nombre> }`, junto a `name` y `epithet`;
  - `content/characters/README.md`.

### Simulador (`crates/sim`)
- **RF15.** `crates/sim` es una lib más un bin (`cargo run -p sim --release -- …` y la receta `just sim *args`). Flags:
  - `--fights N` (por defecto 10000);
  - `--seed <64 hex>` (por defecto, una constante fija);
  - `--content <dir>` (por defecto `content/characters`);
  - `--json`;
  - `--check`: código de salida 1 si falla algún criterio de RF17.

  Esquema:
  - las semillas de cada pelea salen, de 32 en 32 bytes, de un `ChaCha20Rng` sembrado con `--seed`, así que el informe es reproducible;
  - `P` es el número de pares no ordenados de personajes **distintos**, `C(n, 2)`;
  - cada par `{i, j}` usa `m = ⌊N / (2·P)⌋` semillas, y **cada semilla se juega en las dos orientaciones**, así que hay `2·P·m ≤ N` peleas de cruce;
  - además hay `n·m` espejos (cada personaje contra sí mismo), que **no** cuentan en `N` ni en ninguna métrica salvo el sesgo del lado `a`. Todas las demás cifras del informe y de RF17 ("las peleas") son sobre los cruces.

  Informe (texto en inglés, porque es una herramienta de desarrollo y no UI):
  - win rate global por personaje, solo con cruces: `(victorias + empates/2) / apariciones`, en ‰;
  - la matriz de enfrentamientos, con un **aviso** (que no falla) si un cruce queda fuera de 30–70 %;
  - % de victorias del lado `a`, agregado sobre todos los espejos;
  - % de KO, decisión y empate;
  - duración (`end.t`) en p10, p50 y p90;
  - % de peleas con al menos un `moment` y desglose por `kind`, más el % de peleas que gana un lado que estuvo ≥ 250 ‰ por debajo;
  - % de peleas con algún `knockdown`;
  - tiempo total (simulación más `canonical_json` de cada log), medido con `Instant` en el bin, nunca en el motor.

  `print_stdout` se permite solo con un `#[allow]` local en `main.rs`. El CLI nunca recibe semillas de peleas reales.

### Pruebas de determinismo y WASM
- **RF16.** Dorados en `crates/engine/tests/golden.rs`. Los fixtures de personajes viven en `crates/engine/tests/fixtures/`, desacoplados de `content/`, para que balancear no rompa los dorados. Incluyen:
  - (a) al menos 6 vectores `(seed, a, b) → log`, comparados contra `tests/golden/*.json` con `include_bytes!`. Cubren `ko`, `knockdown` y `decision` (con fixtures defensivos), y también `draw` si el plan encuentra una semilla que lo produzca; si no, `draw` se cubre con un test unitario de la regla de RF9;
  - (b) un digest FNV-1a de 64 bits (implementado en el test, sin crate) de la concatenación de `canonical_json` de 1000 peleas con semillas derivadas, comparado con un valor commiteado;
  - (c) `canonical_spec_json` de cada fixture contra su dorado, más la ida y vuelta `from_slice(canonical_spec_json(s)) == s`.

  Los dorados se regeneran con `UPDATE_GOLDEN=1`, solo en nativo.
- **RF17.** Criterios de balance. Los comprueba un test de `crates/sim` sobre el `content/` real, con 10 000 peleas y la semilla por defecto, dentro de `just check`. Es determinista, así que no puede ser *flaky*:
  - el win rate global de cada personaje está entre 400 ‰ y 600 ‰;
  - al menos el 15 % de las peleas tienen algún `moment`;
  - al menos el 80 % de las peleas tienen `end.t` entre 45 000 y 90 000 ms;
  - `time_up` en como mucho el 10 % de las peleas, y `draw` en menos del 1 %;
  - el lado `a` gana entre el 47 % y el 53 % de los espejos;
  - cada `MomentKind` aparece en al menos el 3 % de las peleas, para que ningún momento quede muerto para el narrador (ADR-0010).

  > **Enmienda (006, TA-21).** La fracción en 45–90 s sube de 80 % a **90 %** (`in_band_min_permille: 900`), y `Criteria::RF17` suma los criterios de ritmo de RF12 de la [spec de 006](../006-ritmo-rapido/spec.md): separación media entre resoluciones de 1100 a 1300 ms (la banda viene del owner y cambiarla necesita el OK del product-owner), ≥ 250 ‰ de separaciones en ráfaga (≤ 900 ms), mediana de ≥ 2 pausas (≥ 2500 ms) por pelea, ≥ 900 ‰ de peleas con alguna pausa y separación mínima ≥ `TIMELINE_CONTRACT.strike_gap_ms`. El game-designer puede ajustar la ráfaga, la mediana de pausas y las peleas con pausa.

  El game-designer puede ajustar en la fase B la fracción del 80 %, el tope de `time_up`, la banda del sesgo y el 3 % por `MomentKind`, que son valores iniciales de esta spec. La banda de 45 000–90 000 ms viene del intent y de 002 y no se ajusta en esta fase, y subir el tope de `time_up` por encima del 10 % necesita además el OK del product-owner (preocupación 7). Todo ajuste queda en `Criteria::RF17` de `sim` y en esta spec.
- **RF18.** WASM:
  - `rust-toolchain.toml` agrega los targets `wasm32-unknown-unknown` y `wasm32-wasip1`, y `deny.toml` los agrega a `[graph] targets`;
  - `check-rust` corre `cargo clippy -p engine -p engine-wasm --target wasm32-unknown-unknown --locked -- -D warnings`, que demuestra que el motor y los bindings compilan para el navegador;
  - `check-rust` corre también `cargo test -p engine --test golden --target wasm32-wasip1 --locked`, con `CARGO_TARGET_WASM32_WASIP1_RUNNER` apuntando a `scripts/wasi-runner.mjs`. El runner usa `node:wasi`, sin wasmtime ni crates nuevos, sin `preopens` y con `env: {}`. Así, los **mismos** dorados de RF16 pasan en nativo y en WASM, y la igualdad byte a byte queda demostrada contra los mismos archivos (con `include_bytes!` no hace falta sistema de archivos).

  El runner convierte un trap (`panic = abort`) en el código de salida 101.
- **RF19.** `crates/engine-wasm` (mínimo) es un `cdylib` con `wasm-bindgen` (`=0.2.129`, ya en el lock). Exporta una sola función:
  ```rust
  simulate(seed_hex: &str, a_json: &str, b_json: &str, ruleset_version: u16) -> Result<String, JsError>
  ```
  - **Entrada:**
    - `a_json` y `b_json` son exactamente la forma motor de `CharacterSpec`, con `deny_unknown_fields`;
    - cada argumento de texto se rechaza **antes de parsear** si pasa de 8 KiB (`seed_hex` debe tener exactamente 64 caracteres).
  - **Salida:** `canonical_json` como `String`.
  - **Errores:** el `JsError` lleva **solo** uno de estos códigos y nunca el mensaje de `serde_json`:
    - `input.too_large`, `seed.invalid_hex`, `spec.invalid_json`, `ruleset.unknown` y `log.encode_failed`;
    - `spec.<código>` por cada variante de `SpecError` (la lista exacta sale de ese enum, y un test comprueba que cubre todos los códigos).
  - **Lints:** `engine-wasm` usa `[lints] workspace = true`, así que hereda `unsafe_code = "forbid"`, y no lleva **ningún** `allow(unsafe_code)`. Bajo `forbid`, el compilador rechaza cualquier `allow` con `E0453`. `scripts/check-engine.mjs` falla si en `crates/engine-wasm`:
    - `src/` contiene el token `\bunsafe\b`;
    - aparece **algún** atributo que relaje `unsafe_code` (`allow`, `expect` o `warn`), ya sea directo, de crate (`#![…]`), dentro de un `cfg_attr(…)` o en el cuerpo de una macro (`macro_rules!`);
    - `src/` contiene `include!` o un atributo `#[path`, porque meterían en el crate código que el chequeo no revisa;
    - `Cargo.toml` no tiene `[lints] workspace = true` o tiene una tabla `[lints.*]` propia.

  En 003 no hay glue JS ni la usa la web: la ejecución en navegador es parte de la feature de verificación de repeticiones.

### Seguridad
- **RF20.** El motor no registra ni serializa la semilla y no la incluye en `FightLog` (RF4), y no tiene dependencias de logging (RF5). Lo comprueban un test exacto, `format!("{:?}", seed) == "Seed(<redacted>)"`, y `scripts/check-engine.mjs`.
- **RF21.** `simulate` es total con entradas no confiables, porque `engine-wasm` recibe JSON del cliente:
  - la validación rechaza lo que esté fuera de rango;
  - el bucle termina, porque cada acción avanza el reloj del atacante por lo menos `interval_min + windup_strike` = 1200 ms;
  - el log está acotado por RF2: cada lado hace como mucho 68 acciones (`⌊(84 900 − 3400) / 1200⌋ + 1`), cada acción aporta como mucho 3 eventos (`attack`, la resolución y `knockdown`) y la pelea suma como mucho 10 eventos únicos (`intro`, 2 `low_hp`, 2 `comeback`, `decisive_crit`, `clutch_ko`, `ko`, `time_up` y `end`), así que la cota es 418 < 512. El test de cotas de `Ruleset` la calcula con los campos del ruleset, y si se alcanzara `max_events`, `simulate` devuelve `SpecError::EventLimit`;
  - no hay pánicos, por los lints de RF5 y los del workspace.

  Un test alimenta `simulate` con specs inválidas y con extremos válidos (todos los stats en 1, en 100 o en el presupuesto exacto) y espera `Err` o un log que cumpla RF2.
- **RF22 (requisito duro entre features, dueño: TA-13 (Linear)).** Ninguna pelea de producción abre apuestas si no se cumple **una** de estas dos condiciones:
  - (a) los `canonical_spec_json` de los dos personajes y la `ruleset_version` se publican como snapshot inmutable al abrir las apuestas, junto con el commit;
  - (b) el commit se extiende a `SHA-256(seed ‖ canonical_spec_json(a) ‖ canonical_spec_json(b) ‖ ruleset_version)`, lo que es una enmienda de ADR-0002 con el OK del owner.

  TA-13 también cubre la confidencialidad del `FightLog` hasta `Settled`. 003 aporta lo necesario para las dos opciones (RF4, RF11 y RF16 c), y el requisito se agrega a "Prerrequisitos del primer despliegue" de `docs/architecture.md`.

## Requisitos no funcionales
- **Rendimiento:** 10 000 simulaciones, con `canonical_json` incluido, en menos de 10 s con `--release` en la Mac del owner, según el tiempo que reporta `sim`. CI no lo asegura, porque el runner es ruidoso. En producción, el motor corre una vez por pelea al cerrar las apuestas: microsegundos y KB en el VPS de 1 vCPU / 4 GB.
- **`overflow-checks = true`** en `[profile.release.package.engine]`: un desbordamiento entra en pánico en lugar de divergir en silencio entre perfiles. Las fórmulas están acotadas, y los `u32`/`u64` internos no se desbordan con stats ≤ 100.
- **i18n:** el log solo lleva códigos. Los nombres de los personajes y de las habilidades viven en `lore.<locale>.md` (RF14), y su parseo y su validación en CI corresponden a la feature que los muestre.
- **Dependencias:** no se agrega ninguna versión nueva al lock. `rand_chacha 0.9.0` (con `rand_core 0.9.5` y `ppv-lite86 0.2.21`), `wasm-bindgen 0.2.129`, `serde` y `serde_json` ya están en él. Aun así, se corre `just crate-age` y se agregan `engine`, `engine-wasm`, `content` y `sim` a `WORKSPACE_MEMBERS` de `scripts/check-crate-age.mjs` y a `[workspace].members`.

## Diseño

### Componentes afectados
| Pieza | Cambio |
|---|---|
| `crates/protocol` | Nuevo `src/fight.rs` con los tipos del log y sus tests de forma. `just gen` agrega los `.ts` a `packages/protocol-ts`. |
| `crates/engine` (nuevo) | `simulate`, `Seed`, `Ruleset`, `CharacterSpec`, `SpecError`, `EncodeError`, `canonical_json` y `canonical_spec_json`. Depende de `protocol` (sin `ts`), `serde`, `serde_json` y `rand_chacha`. |
| `crates/engine-wasm` (nuevo) | `cdylib` con `wasm-bindgen`. Depende de `engine` y `serde_json`. |
| `crates/content` (nuevo) | Lee y valida `content/characters/`. Depende de `engine`, `serde`, `serde_json` y `thiserror`. Lo usarán `sim` y, más adelante, el server. |
| `crates/sim` (nuevo) | Lib y bin del simulador. Depende de `engine`, `content`, `serde`, `serde_json`, `thiserror` y `rand_chacha` (sin features, para derivar las semillas). Va en `crates/` y no en `tools/` porque el filtro `rust` de CI cubre `crates/**`. |
| `content/characters/*` | De 2 a 4 personajes (fase B). |
| `rust-toolchain.toml`, `Cargo.toml`, `deny.toml`, `justfile`, `scripts/{wasi-runner,check-engine,check-crate-age}.mjs` (con tests en `scripts/__tests__/`) | Targets, miembros y recetas (`sim`, `_check-wasm`, `_check-engine`, conectadas a `check-rust`). |
| Docs | Al cerrar: `docs/architecture.md` (firma `-> Result`, nuevos crates, serialización canónica y RF22 en los prerrequisitos de producción), el skill `write-character` y `content/characters/README.md`. |

Dirección de dependencias: `protocol ← engine ← {content, engine-wasm, sim}` (y el server, en el futuro). El motor depende de `protocol` sin la feature `ts`, y `_check-ts-rs-graph` se extiende a `engine`. No se elige "el motor define y `protocol` reexporta" porque obligaría a `protocol` a depender del motor y a meter `ts-rs` en el motor para exportar los tipos.

### Modelo de datos / tipos (protocol)
```rust
// crates/protocol/src/fight.rs — every type: Debug, Clone, PartialEq, Eq, Serialize, Deserialize,
// #[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]

#[serde(rename_all = "snake_case")]
pub enum Side { A, B }                                   // "a" | "b"

#[serde(rename_all = "snake_case")]
pub enum AbilityKind { Strike, Heavy, Special }

#[serde(rename_all = "snake_case")]
pub enum MomentKind { Comeback, DecisiveCrit, ClutchKo }

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
/// always the side opposite to `attacker`. `damage` is the damage applied (never more than
/// the target's previous hp); `hp` is the target's hp afterwards.
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FightEvent {
    Intro     { t: u32 },
    Attack    { t: u32, attacker: Side, ability: String, kind: AbilityKind },
    Hit       { t: u32, attacker: Side, ability: String, damage: u16, crit: bool, weak: bool, hp: u16 },
    Block     { t: u32, attacker: Side, ability: String, damage: u16, hp: u16 },
    Dodge     { t: u32, attacker: Side, ability: String },
    Knockdown { t: u32, side: Side, duration_ms: u16 },  // side = the one knocked down
    LowHp     { t: u32, side: Side, hp: u16 },
    Moment    { t: u32, kind: MomentKind, side: Side },
    Ko        { t: u32, side: Side },                    // side = the one knocked out
    TimeUp    { t: u32 },
    End       { t: u32, winner: Option<Side>, reason: EndReason },
}
```
Más algunos detalles:
- `ability` es `"strike"` (el golpe básico, que no aparece en el contenido) o el `id` de una habilidad del personaje.
- El `t` de `attack` es el inicio de la acción, y el de `hit`, `block` y `dodge` es el instante del impacto (la resolución).
- `FightLog::outcome() -> Option<(Option<Side>, EndReason)>` lee el `end`; no hay un campo duplicado. Devuelve `None` solo si el log (por ejemplo, uno deserializado) no tiene `end`; para un log del motor siempre es `Some`.
- No se guarda `duration_ms`: la duración es `end.t`.

Ejemplo recortado e ilustrativo. Entre 6800 y 64 200 ms se omitieron eventos, así que los tiempos y los `hp` no cuadran con las fórmulas; una pelea real tiene entre 100 y 300 eventos.
```json
{"ruleset_version":1,"a":{"slug":"rata-electrica","max_hp":720},"b":{"slug":"guerrero-erizado","max_hp":740},"events":[
{"type":"intro","t":0},
{"type":"attack","t":3500,"attacker":"a","ability":"strike","kind":"strike"},
{"type":"hit","t":3900,"attacker":"a","ability":"strike","damage":22,"crit":false,"weak":false,"hp":718},
{"type":"attack","t":4200,"attacker":"b","ability":"spirit_wave","kind":"special"},
{"type":"dodge","t":5300,"attacker":"b","ability":"spirit_wave"},
{"type":"attack","t":5700,"attacker":"a","ability":"thunder_squeak","kind":"special"},
{"type":"hit","t":6800,"attacker":"a","ability":"thunder_squeak","damage":104,"crit":true,"weak":false,"hp":614},
{"type":"knockdown","t":6800,"side":"b","duration_ms":1500},
{"type":"hit","t":40200,"attacker":"b","ability":"rising_fist","damage":61,"crit":false,"weak":false,"hp":170},
{"type":"low_hp","t":40200,"side":"a","hp":170},
{"type":"block","t":51900,"attacker":"b","ability":"rising_fist","damage":14,"hp":156},
{"type":"moment","t":58300,"kind":"comeback","side":"a"},
{"type":"attack","t":63100,"attacker":"a","ability":"thunder_squeak","kind":"special"},
{"type":"hit","t":64200,"attacker":"a","ability":"thunder_squeak","damage":96,"crit":true,"weak":false,"hp":0},
{"type":"moment","t":64200,"kind":"decisive_crit","side":"a"},
{"type":"ko","t":64200,"side":"b"},
{"type":"end","t":68200,"winner":"a","reason":"ko"}]}
```
Una pelea que llega al tope termina así: `{"type":"time_up","t":86000}` y `{"type":"end","t":90000,"winner":"b","reason":"decision"}`. `canonical_json` no tiene saltos de línea; aquí se agregaron para leerlo.

**Orden dentro de un mismo `t`**, al resolver un ataque:
1. `hit`, `block` o `dodge`;
2. `knockdown`;
3. `low_hp`;
4. `moment` `comeback`;
5. y, si hay KO: `moment` `decisive_crit`, `moment` `clutch_ko` y `ko`.

Para el narrador (ADR-0010), los triggers se derivan del evento:
- inicio: `intro`;
- crítico: `hit` con `crit`;
- esquiva: `dodge`;
- caída: `knockdown`;
- HP bajo: `low_hp`;
- remontada: `moment` `comeback`;
- KO: `ko`;
- decisión o empate: `time_up` y `end`.

`clutch_ko` es un KO "al borde" calculado dentro de la pelea. **No** es el "favorito perdiendo" de ADR-0010: ese depende de los pozos, y lo calcula el server o el narrador.

### Correspondencia con la `Timeline` v1 de 002
El adaptador es `timelineFromFightLog` (RF-C1 de 002), del lado del consumidor.

| `FightEvent` | Cue de la `Timeline` |
|---|---|
| `intro` | `intro` en 0 |
| `attack` | Ninguno. Aporta el `move`: `strike` → `attack_light`, `heavy` → `attack_heavy` y `special` → `special`. El windup que se ve lo pone el renderer (≤ 400 ms antes del impacto), no el windup simulado del motor (400/700/1100 ms), que solo espacia la pelea. |
| `hit` con `crit = false` / `crit = true` | `strike` con `result: hit` / `result: crit`, en `at_ms = t`, con el `move` del `attack` previo, `damage` y `target_hp = hp` |
| `block` | `strike` con `result: block`, `damage` y `target_hp = hp` |
| `dodge` | `strike` con `result: dodge`, `damage: 0` y `target_hp` igual al anterior |
| `knockdown` | `knockdown` (`fighter = side`, `duration_ms`) |
| `low_hp`, `moment` | Ninguno: los usan el HUD o el narrador |
| `ko` | `ko` |
| `end` con `reason: ko` | `win` del ganador en `ko.t + 1500`, y `duration_ms = end.t`. Con `end_ms = 4000` quedan ≥ 2 s de pose. |
| `time_up`, `end` con `decision` o `draw` | **No expresable en v1 →** `timeline_version` 2 (ver la preocupación 7) |

### Modelo de simulación (ruleset v1)
Cada fila es un campo de `Ruleset` con su valor v1. Los valores se afinan en la fase B con `sim`, y cambiar la forma de una fórmula enmienda esta spec. En la tabla, "‰" es sobre 1000 y todas las divisiones son enteras, en el orden escrito. Las cifras de esta tabla se midieron con `sim` (`just sim`, motor real y los cuatro homenajes de `content/`, semilla raíz por defecto): 9996 cruces (6 pares, 833 semillas por par, en las dos orientaciones) y 3332 espejos:
- win rate por personaje entre 47,7 y 51,1 % (cruces de 41,7 a 58,2 %);
- 94,6 % de las peleas en 45–90 s (p10/p50/p90 de `end.t` = 46,8/53,9/63,5 s);
- momentos en el 38,0 % (comeback 13,3, decisive_crit 11,2 y clutch_ko 23,6 %);
- 100 % de KO, 0 % de `time_up` y 0 % de `draw`;
- separación mínima entre impactos de 700 ms (invariante de RF2, no una medición);
- 49,8 % de victorias del lado `a` en los espejos.

> **Enmienda (006, TA-21).** Con el respiro, las cifras de arriba son históricas. `just sim` (ruleset v1 reescrito, mismos cruces, espejos y semilla) mide ahora (RF13 de la [spec de 006](../006-ritmo-rapido/spec.md)):
> - win rate por personaje entre 48,9 y 51,1 % (cruces de 39,3 a 60,6 %);
> - 98,6 % de las peleas en 45–90 s (p10/p50/p90 de `end.t` = 50,6/59,4/70,6 s);
> - momentos en el 38,1 % (comeback 13,2, decisive_crit 11,5 y clutch_ko 23,1 %);
> - 99,8 % de KO, 0,1 % de decisión, 0,1 % de `time_up` y 0,0 % de `draw`;
> - 36/43/51 resoluciones por pelea (p10/p50/p90), con una separación media de 1226 ms y mínima de 700 ms;
> - 28,5 % de separaciones en ráfaga y 6,2 % en pausa, con 1/3/4 pausas por pelea (p10/p50/p90) y el 98,9 % de las peleas con alguna pausa;
> - 49,3 % de victorias del lado `a` en los espejos (49,40 / 50,26 / 50,14 % con tres semillas raíz).

Antes de la fase B, estas cifras salían de la reimplementación aproximada en Python del game-designer (ver la Revisión).

| Concepto (campos) | Regla v1 |
|---|---|
| Presupuesto de stats (`stat_budget`) | `hp + attack + defense + speed + special ≤ 300`, cada uno en 1–100 |
| HP máximo (`hp_base`, `hp_per_point`) | `max_hp = 500 + 4·hp` |
| Intervalo entre acciones (`interval_base_ms`, `interval_per_speed_ms`, `jitter_ms`) | `interval = 1800 − 10·speed` ms, más un jitter de `roll(jitter_ms + 1)` = `roll(201)` |
| Primer turno (`intro_ms`) | `next_at = 3000 + interval/2` |
| Windup (`windup_strike_ms`, `windup_heavy_ms`, `windup_special_ms`) | De `attack` a la resolución: 400, 700 y 1100 ms |
| Próximo turno del atacante | `t_res + interval + jitter` |
| Recuperación del defensor (`recovery_ms`) | En golpe, bloqueo y esquiva: `next_at_def = max(next_at_def, t_res + 300)` |
| Knockdown (`knockdown_min_permille`, `knockdown_ms`) | Un `hit` crítico que deja al objetivo con `hp > 0` y aplica `damage·1000 ≥ 100·max_hp_def` emite `knockdown` de 1500 ms. Además, `next_at_def = max(…, t + 1500 + 300)` y `next_at_atk = max(…, t + 1500)`, así que nadie golpea durante la caída. |
| Elección de la acción (`w_strike_base`, `w_heavy_base`, `w_special_base`, `w_special_per_show`) | `strike`: `100 + aggression`. Cada `heavy` disponible: `50 + aggression`. Cada `special` disponible: `30 + 2·showmanship`, ×2 si el luchador está desesperado. Se recorren en orden fijo (primero `strike` y después las habilidades en el orden del archivo) y se elige el primero cuyo peso acumulado supere `roll(total)`. |
| Enfriamiento | Después de usar una habilidad, queda bloqueada durante sus siguientes `cooldown` acciones propias |
| Defensa (`dodge_base`, `dodge_per_speed`, `block_base`, `block_defense_div`), con `r = roll(1000)` | `dodge‰ = 40 + 2·speed_def`. `block‰ = 60 + defense_def/2 + caution_def`. Si `r < dodge`, esquiva; si `r < dodge + block`, bloqueo; si no, golpe. |
| Daño base (`strike_power`, `damage_offset`) | `base = power·(100 + off)/(100 + def)`. Para `strike` (potencia 20) y `heavy`: `off = attack_atk`, `def = defense_def`. Para `special`: `off = special_atk`, `def = special_def`. |
| Desesperación (`desperation_permille`, `desperation_crit_bonus`, `desperation_dmg_num/den`) | Si el atacante tiene `hp·1000/max_hp ≤ 250` al resolver: crítico +150 ‰ y daño ×5/4 (solo en golpe) |
| Crítico (`crit_base`, `crit_per_special`, `crit_num/den`) | Solo en golpe: `roll(1000) < 40 + special_atk (+150 si está desesperado)`, ×3/2 |
| Debilidad (`weak_num/den`) | Si la habilidad tiene `element` y ese elemento está en las `weaknesses` del objetivo: `weak = true`, ×5/4 |
| Bloqueo (`block_chip_num/den`) | ×3/10, sin crítico ni desesperación |
| Orden del daño | **Golpe:** `d = base·(85 + roll(31))/100`, después desesperación, crítico y debilidad, y por último `d = max(d, 1)`. **Bloqueo:** varianza, debilidad y ×3/10. En los dos casos, el `damage` aplicado es `min(d, hp_antes)`. |
| Tiempo (`time_limit_ms`, `end_ms`) | `time_limit_ms = 86 000` y `end_ms = 4000`, así que `end.t ≤ 90 000` |
| HP bajo (`low_hp_permille`) | La primera vez que `hp·1000/max_hp ≤ 250` con `hp > 0`, una vez por lado |
| Momentos (`comeback_deficit_permille`, `clutch_permille`) | 250 y 60 (ver abajo) |
| Tope de eventos (`max_events`) | 512 |
| Respiro (`breather_after_resolutions`, `breather_chance_permille`, `breather_min_ms`, `breather_span_ms`) | Tras cada acción sin KO: si hubo knockdown, `since_breather = 0` y no se sortea nada; si no, `since_breather += 1`, y con `since_breather ≥ 9`, `roll(1000) < 200` abre un respiro de `d = 1800 + roll(801)` ms (1800–2600): `next_at_a += d`, `next_at_b += d` (con `saturating_add`) y `since_breather = 0`. Desplaza los dos relojes por igual, así que no cambia quién actúa; no emite eventos. Derivados: `strike_gap_min() = min(recovery_ms, interval_min) + windup_strike_ms` = 700 ms y `breather_gap_min() = breather_min_ms + strike_gap_min()` = 2500 ms. |

> **Enmienda (006, TA-21).** Fila "Respiro" nueva (RF1 y RF2 de la [spec de 006](../006-ritmo-rapido/spec.md)). Ningún otro valor de la tabla cambia.

Un test unitario (`v1_respects_the_bounds`) comprueba sobre `Ruleset::v1()` las cotas que el ritmo de RF2 necesita, contra `TIMELINE_CONTRACT` y sin literales:
- `end_ms ≥ win_gap_ms + win_pose_ms` (3500);
- `intro_ms + windup_strike_ms ≥ intro_gap_ms` (2000);
- `strike_gap_min() ≥ strike_gap_ms` (600), que reemplaza a `recovery_ms + windup_strike_ms ≥ 600` y a `interval_min + windup_strike_ms ≥ 600`;
- `knockdown_min_ms ≤ knockdown_ms ≤ knockdown_max_ms` (800–3000);
- `breather_after_resolutions ≥ 1`, `breather_chance_permille ≤ 1000` y `breather_span_ms + 1` es una cota válida de `roll`;
- `time_limit_ms + interval_base_ms + jitter_ms + breather_min_ms + breather_span_ms + knockdown_ms + recovery_ms` cabe en `u32`;
- `max_events_bound()` sigue en 418 (el respiro solo resta acciones).

> **Enmienda (006, TA-21).** Lista de cotas según RF6 de la [spec de 006](../006-ritmo-rapido/spec.md). El umbral de pausa (2500) no se comprueba aquí: es una definición de `sim`, que lo ata a `breather_gap_min()` con su propio test.

**Momentos.** Se emiten como `moment`, cada `kind` como mucho una vez por lado y por pelea:
- **`comeback`**: un lado que en algún momento estuvo por debajo del rival en ≥ 250 ‰ de HP relativo (`hp·1000/max_hp`) pasa a estar estrictamente por encima. Se emite al resolver el ataque que lo provoca, aunque ese lado no termine ganando.
- **`decisive_crit`**: un crítico produce el KO y el mismo golpe sin el ×3/2 (pero con la desesperación y la debilidad) no lo habría producido: `d_sin_crit < hp_antes`. El `side` es el atacante, que es el ganador.
- **`clutch_ko`** ("sorpresa", un KO al borde): el ganador da el KO con `hp·1000/max_hp ≤ 60`. El `side` es el ganador.

### Esquema de personaje
`content/characters/<slug>/character.json`. El archivo es el de abajo, y la **forma motor** de `engine::CharacterSpec` es ese mismo objeto sin `archetype`, `ruleset_version` ni `rivals`, con los campos en este orden. La forma motor es la que serializa `canonical_spec_json` y la única que acepta el WASM.
```json
{
  "slug": "rata-electrica",
  "archetype": "electric-rodent",
  "ruleset_version": 1,
  "stats": { "hp": 55, "attack": 60, "defense": 45, "speed": 85, "special": 55 },
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
| `abilities[].element` | Opcional, de la lista cerrada `fire`, `water`, `electric`, `ground`, `air` e `ice`. En la forma canónica se serializa como `null` si falta. |
| `weaknesses` | De 0 a 2 elementos únicos de esa misma lista |
| `rivals` | Slugs existentes en el roster, para el narrador; el motor los ignora |

Se eligió JSON con `serde_json`, que ya está en el workspace, para no agregar dependencias. YAML exigiría un crate nuevo (`serde_yaml` está archivado), y TOML sumaría cinco crates. No hay floats, así que el parseo es exacto.

### API / mensajes WS
No hay. El envío progresivo de eventos y del `end` lo define la spec del server y del scheduler (fuera de alcance, TA-13).

### Flujo
```
content/characters/*/character.json ──content::load_roster──▶ Vec<CharacterSpec> (sorted by slug)
                                                                    │
Seed (32 B, server or sim) ──▶ engine::simulate(seed, a, b, Ruleset::for_version(1)) ──▶ FightLog
                                                                    │
                       canonical_json / canonical_spec_json ──▶ bytes (DB, golden, WASM, verification)
```

## Áreas de preocupación
1. **Firma de ADR-0001.** El ADR escribe `simulate(...) -> FightLog`, y aquí devuelve `Result<FightLog, SpecError>` para que `engine-wasm` reciba entradas no confiables sin pánicos (RF21). Es un refinamiento, no una contradicción: la simulación de entradas válidas sigue siendo infalible. Se refleja en `docs/architecture.md` y no hace falta otro ADR.
2. **Commit-reveal incompleto (ADR-0002), TA-13 (Linear).** `SHA-256(seed)` solo compromete la semilla. Para re-simular hacen falta además los `CharacterSpec` exactos y la `ruleset_version`, y si el operador pudiera cambiar un spec después del commit, influiría en el resultado. El contenido puede cambiar entre temporadas, así que el slug no basta. RF22 lo convierte en un **requisito duro** que bloquea producción, y 003 entrega la forma canónica que necesita. La elección entre el snapshot (a) y el commit extendido (b) es de la spec del server o scheduler, en TA-13.
3. **El `FightLog` completo es tan sensible como la semilla hasta `Settled`**, porque contiene el resultado. El motor no puede protegerlo: lo hace cumplir el server (ADR-0002, punto 3), y su spec debe tratar `FightLog` y `outcome()` con la misma regla de no serialización que la semilla. Está registrado en TA-13.
4. **`wasm-bindgen` frente a `unsafe_code = "forbid"`.** Resuelto con `forbid` en los dos crates (enmienda del 2026-10-27). En la implementación se comprobó que el shim de `wasm-bindgen 0.2.129` sale de la expansión de una proc-macro externa, donde rustc no reporta `unsafe_code`. `engine-wasm` compila para `wasm32-unknown-unknown` con `forbid` y sin ningún `allow`, y el `.wasm` exporta `simulate`. Así que el `allow` que pedía la versión anterior no tenía efecto. Con `forbid` el diseño falla cerrado: si un bump de `wasm-bindgen` generara `unsafe` que el lint sí viera, `_check-wasm` fallaría en ese PR y habría que decidir a conciencia (con una enmienda de esta spec). `scripts/check-engine.mjs` sigue vigilando el token `unsafe` y cualquier atributo que relaje el lint (RF19).
5. **WASI como sustituto del navegador.** El test byte a byte corre en `wasm32-wasip1`, no en el artefacto `wasm32-unknown-unknown` de `wasm-bindgen`. Es el mismo backend wasm32 y el mismo `usize` de 32 bits, y el motor no toca ninguna API de plataforma, así que la equivalencia es razonable. Aun así, la feature de verificación en navegador debe agregar un test del artefacto real. `node:wasi` está marcado como experimental en Node 22: si su API cambia, solo se rompe el runner de 30 líneas.
6. **Imagen del server.** `apps/server/Dockerfile` copia `rust-toolchain.toml`, y `rustup toolchain install` va a descargar también los dos targets wasm (unas decenas de MB, solo en la etapa builder). Es aceptable. Si devops prefiere evitarlo, el plan de la fase A lo resuelve sin cambiar el canal (`check-compose.mjs` solo enlaza la versión).
7. **Fin por tiempo frente a la `Timeline` v1 de 002 (decisión).** El `FightLog` **conserva** `time_up`, `decision` y `draw`. El log es el registro honesto y verificable de la pelea (ADR-0002), y un motor que "fuerce" el KO antes del tope necesitaría reglas artificiales que también cambian el balance. El motor las hace raras (RF17: como mucho un 10 % de `time_up` y menos de un 1 % de `draw`; con el contenido real, `sim` mide 0 % de `time_up` y de `draw` con el tope de 86 s), pero el formato las expresa. Por eso la ruta de fin por tiempo solo la cubren los dorados del motor (`log-decision-a/b.json`), no el contenido.

   Como la `Timeline` v1 no puede expresarlas, **hace falta `timeline_version` 2**, por RF-C4 de 002:
   - **Dueño:** la fase C de 002 (frontend-engineer), con una enmienda de la spec de 002 que la feature 003 pide formalmente al fusionar su fase A.
   - **Lo que v2 debe expresar:** un cue `time_up`; un `win` sin `ko` previo (decisión); y un cierre sin ganador (empate). Las reglas de pose (≥ 2 s) se mantienen.
   - **Mientras v2 no exista:** el adaptador devuelve `AdapterError` con `adapter.unsupported_ending` para esos logs, sin inventar cues. La primera feature que reproduzca peleas reales en producción no se puede publicar sin v2.
8. **Coordinación con 002.** Ya se incorporaron sus "Datos para 003":
   - duración total de 45 a 90 s, con `end.t` ≤ 90 000;
   - ≥ 2 s de intro;
   - ≥ 600 ms entre impactos (el motor garantiza ≥ 700);
   - knockdown emitido por el motor, no derivado en el adaptador, porque cambia el tiempo de la pelea: nadie golpea durante la caída, y eso solo lo puede garantizar la simulación;
   - ≥ 1,5 s de KO a victoria y ≥ 2 s de pose, gracias a `end_ms = 4000`.

   La densidad la mide `sim`: con el contenido real hay de 36 a 51 resoluciones por pelea (p10 a p90, p50 43), así que se cumplen los ≥ 30 golpes, y la separación media entre resoluciones es de ~1096 ms, por debajo de los ~1,9 s que pidió 002. No es criterio de la fase B: se ajusta con `interval_*`.

   > **Enmienda (006, TA-21).** El owner bajó la referencia a ~1,2 s en RF-B9 de 002, y la media pasó a ser criterio de `sim` (1100–1300 ms, medida en 1226 ms), con el respiro como mecanismo de las pausas (ver la [spec de 006](../006-ritmo-rapido/spec.md)). La separación que fije el owner en RF-B9 de 002 entra como un ajuste de valores. El cue `taunt` (personalidad) queda como extensión futura: entraría con un evento nuevo, una `ruleset_version` nueva y `timeline_version` 2 o 3, a pedido de game-designer.
9. **Empates y apuestas.** Los produce un `time_up` con HP relativo idéntico, y deben ser menos del 1 % (RF17). En un empate lo natural es un reembolso, y, como todo movimiento de monedas, se hace con asientos del ledger (ADR-0005), nunca tocando saldos. Lo decide la spec de apuestas.
10. **Debilidades en v0 y balance.** Con solo 2–4 personajes, una debilidad (×5/4) puede inclinar mucho un cruce. El criterio de 40–60 % es global, y una matriz asimétrica es aceptable e interesante para el parimutuel. `sim` avisa, sin fallar, de los cruces fuera de 30–70 %.
11. **El balance dentro de `just check`.** Un cambio de contenido que desbalancee rompe el check. Es intencional (lo dice el skill `write-character`).

## Respuestas a preguntas abiertas del intent
- **P: ¿Por turnos con tiempo simulado o con ticks continuos?** → R: Turnos con tiempo simulado (RF8). Cada acción tiene `next_at` en ms enteros, el más próximo actúa y las acciones nunca se solapan. Hay menos estado y menos consumo de PRNG que con ticks, el orden es trivialmente determinista y el renderer recibe una secuencia limpia.
- **P: ¿Cómo influye la personalidad?** → R: Como pesos sobre las acciones, con una sola tirada ponderada por turno. `aggression` sube `strike` y `heavy`, `showmanship` sube `special` (el doble en desesperación) y `caution` sube el bloqueo. No hay árbol de comportamiento en v0.
- **P: ¿Debilidades por tipo en v0?** → R: Sí, mínimas: seis elementos cerrados, un `element` opcional por habilidad, hasta dos debilidades y ×5/4, sin resistencias ni estados. ADR-0001 ya las incluye en el personaje, y dan material al narrador (`weak`) por poco costo.
- **Duración de 45 a 90 s, a confirmar con 002** → Es `end.t` total, intro y pose incluidas (RF2 y RF17). `intro_ms`, `time_limit_ms` y `end_ms` son campos de `Ruleset`. Si 002 pide otro ritmo, se cambian los valores y se regeneran los dorados, mientras la versión siga siendo pre-producción.

## Estrategia de pruebas
- **Unit (`engine`):**
  - `roll` (rechazo y límites, con `NonZeroU32`);
  - la validación de `CharacterSpec`, con cada código de `SpecError`;
  - cada fórmula de la tabla y su orden;
  - el knockdown;
  - los tres momentos, con escenarios construidos;
  - la regla de `decision`/`draw`;
  - las cotas de `Ruleset::v1()`;
  - `format!("{:?}", seed) == "Seed(<redacted>)"`.
- **Forma (`protocol`):** JSON exacto de cada variante y de `FightLog`, y rechazo de un `type` desconocido o de campos que falten.
- **Propiedad (`engine`, sin crates nuevos):** 1000 semillas derivadas por cada par de fixtures. Cada log simulado dos veces da los mismos bytes y cumple todas las invariantes de RF2 (incluidas las de ritmo). Más los extremos de RF21.
- **Dorados (nativo y WASI):** RF16 y RF18, dentro de `just check` y del job `rust` de CI (que ya tiene Node).
- **`engine-wasm` (nativo):** cada código de error de RF19, incluidos `input.too_large` y un JSON con un campo de más, y que ningún error contenga texto de `serde_json`.
- **Integración (`content`):** fixtures inválidos (un campo desconocido, un slug distinto del directorio, un presupuesto excedido o un lore que falta) y el `content/` real (RF13).
- **Scripts (Vitest):** `check-engine.mjs`, con casos negativos (`getrandom` en el grafo, `rand_core` con `os_rng`, un token `unsafe`; un `allow`, `expect` o `warn` de `unsafe_code` directo, de crate, dentro de `cfg_attr` o en una macro; `include!`; `#[path`; y un `Cargo.toml` sin `[lints] workspace = true` o con una tabla `[lints.*]` propia), y `wasi-runner.mjs`.
- **Balance (`sim`):** RF17 sobre el `content/` real. El tiempo de RF-NF se mide a mano con `just sim --release` y se pega en el PR de la fase B.
- **Grafo:** `_check-engine` y `_check-ts-rs-graph`, extendido a `engine`.

## Alternativas consideradas
- **Ticks de tiempo continuo:** más estado, más tiradas por segundo, más difícil de razonar y de balancear, y sin beneficio visual, porque el renderer coreografía igual.
- **Árbol de comportamiento para la personalidad:** más expresivo, pero cuesta más diseñarlo y balancearlo. Los pesos bastan para v0.
- **Que el motor siempre termine en KO:** simplificaría la `Timeline`, pero exige reglas artificiales (daño creciente o muerte súbita) que deforman el balance y el registro. Descartado (preocupación 7).
- **Knockdown derivado en el adaptador:** el adaptador no puede garantizar que nadie golpee durante la caída sin mover golpes, lo que rompería el `target_hp` y la sincronía. Descartado.
- **`FightEvent` en `engine` reexportado por `protocol`:** invierte la dependencia y mete `ts-rs` en el motor. Descartado.
- **El evento como `{ t, kind: {...} }` o con `#[serde(flatten)]`:** el anidado es menos cómodo para el consumidor, y `flatten` tiene rarezas en serde y en ts-rs. Repetir `t` en cada variante es explícito y genera una unión TS limpia.
- **`damage` sin recortar (daño excedente):** lo descarta la regla `target_hp = anterior − damage` de 002 y la invariante de RF2. `decisive_crit` ya usa el daño sin recortar internamente.
- **YAML o TOML para los personajes:** dependencias nuevas, y en YAML, crates sin mantenimiento.
- **`wasm-bindgen-cli` + Node, o wasmtime, para el test WASM:** hay que instalar una herramienta de versión acoplada en local y en CI. WASI con `node:wasi` solo añade el target.
- **Exportaciones crudas sin `wasm-bindgen`:** exigen `unsafe` escrito a mano.
- **`rand::distributions` / `gen_range`:** su algoritmo no tiene garantía de estabilidad entre versiones. Se usa un `roll` propio sobre `next_u32`.

## Revisión

### product-owner — 2026-10-26 — REQUEST_CHANGES
- Verifiqué: frontmatter válido; la spec resuelve el problema del intent (impredecible pero reproducible, balanceable, eventos suficientes para renderer y narrador); cada criterio de éxito tiene requisito y prueba (RF16/RF18 byte a byte, RF17 win rate y momentos, RNF de 10k < 10 s); las tres preguntas abiertas están respondidas; alcance sin scope creep (server, apuestas, commit-reveal completo y reembolsos se derivan a sus specs); coherencia con ADR-0001, 0004, 0005, 0007 y 0010; ritmo frente a "Datos para 003" de 002 (≥ 2 s de intro, ≥ 700 ms entre impactos, ≥ 1500 ms de KO a fin, look-ahead de 400 ms).
- Cambios pedidos:
  1. **Techo de duración.** RF17 acepta `end.t` hasta 92 000 ms y 002 suma ≥ 2 s de pose de victoria: una pelea por decisión dura ~94 s en pantalla, fuera de los 45–90 s del intent. Bajar `time_limit_ms` (p. ej. 86 000) y ajustar RF17, o definir qué cuenta como duración de reproducción y registrar la desviación.
  2. **Peleas sin KO frente a la Timeline v1 de 002**, que exige exactamente un `ko` y un `win` y no tiene cue para `time_up`, `decision` ni `draw`. Completar la tabla de correspondencias (`time_up`, `end` con `decision`/`draw`, y `hit` con `crit = true` → `result: crit`), dejar dicho que exige `timeline_version` 2 (RF-C4 de 002) y quién es su dueño, o decidir que el motor no produce peleas sin KO.
- No bloqueante: daño excedente vs. `target_hp` de la Timeline (recortar en el adaptador o relajar 002); rango orientativo por cruce, informativo en `sim`; aclarar que `sudden_ko` no es el "favorito perdiendo" de ADR-0010.

### security-reviewer — 2026-10-26 — REQUEST_CHANGES
- Verifiqué: frontmatter; coherencia con ADR-0001 (el `-> Result` es aceptable) y ADR-0002; `Seed` sin `Serialize` y con `Debug` redactado; el `Debug` de `ChaCha20Rng` en `rand_chacha 0.9.0` no filtra la semilla, pero su `Serialize` (feature `serde`, hoy inactiva) sí la incluye; ninguna versión nueva entra al lock (`rand_chacha 0.9.0`, `rand_core 0.9.5`, `ppv-lite86 0.2.21`, `wasm-bindgen 0.2.129`); por unificación de features `rand_core` se compila hoy con `os_rng`/`getrandom 0.3.4` (vía `tungstenite → rand/std`); cotas de terminación (~270 eventos en el peor caso < tope 512, sin divisiones por cero ni desbordes con stats ≤ 100); `wasm-bindgen-macro-support 0.2.129` traslada los atributos de la función al shim generado, así que `deny` + `allow(unsafe_code)` en el item exportado es viable.
- Cambios pedidos:
  1. **Commit-reveal sin `CharacterSpec`:** (a) registrado en Linear como TA-13 (incluye la confidencialidad del `FightLog` hasta `Settled`); enlazarlo desde la preocupación 2; (b) convertir "valorar" en requisito duro: ninguna pelea de producción abre apuestas sin snapshot inmutable publicado o commit extendido (enmienda de ADR-0002 con OK del owner); (c) en 003, `CharacterSpec` con `Serialize` + `Deserialize` y `deny_unknown_fields`, forma canónica (`canonical_spec_json`), test de ida y vuelta y un dorado; el WASM acepta exactamente esa forma.
  2. **RF21 "sin pánicos":** `#![deny(clippy::indexing_slicing, clippy::panic, clippy::unreachable, clippy::todo, clippy::unimplemented)]` en `engine` y `engine-wasm`; `roll` recibe `NonZeroU32` y calcula 2³² en `u64`; `canonical_json` sin `unwrap_or_default()` (devuelve `Result` o documenta cómo descarta el error sin `expect`).
  3. **`_check-engine-graph` debe cubrir features:** `crates/engine/clippy.toml` con `disallowed-methods` (`SeedableRng::{from_os_rng, try_from_os_rng, from_rng, seed_from_u64}`) y `disallowed-types` (`OsRng`); el chequeo falla si `rand_chacha`/`rand_core` tienen `serde` u `os_rng` en el grafo del motor; test negativo del script; documentar la resolución de features usada.
  4. **`allow(unsafe_code)` en `engine-wasm`:** `_check-wasm` falla si `crates/engine-wasm/src` contiene el token `unsafe` o hay más de un `allow(unsafe_code)`; usar `allow`, no `expect`; la tabla `[lints]` de `engine-wasm` replica la del workspace salvo `unsafe_code = "deny"` y una prueba de `scripts/` lo compara.
- Recomendaciones: targets wasm en `[graph] targets` de `deny.toml`; test exacto `format!("{:?}", seed) == "Seed(<redacted>)"` y declarar que `Seed` no implementa `Display`/`LowerHex`/`Serialize` ni expone los bytes; listar todos los códigos de error de RF19, limitar `a_json`/`b_json` (p. ej. 8 KiB) antes de parsear y nunca devolver el mensaje de `serde_json`; `wasi-runner.mjs` sin `preopens` y con `env: {}`; reembolso de `draw` con asientos del ledger (ADR-0005).

### game-designer — 2026-10-26 — REQUEST_CHANGES
- Verifiqué: coherencia con el intent y ADR-0001/0004/0007/0010; respuestas a las preguntas abiertas (las acepto); triggers del narrador cubiertos; mapeo a las animaciones de 002 (`low_hp` a 250 ‰ = su "HP < 25 %"); el esquema de personaje alcanza para 2–4 homenajes distintos. Reimplementé la tabla del ruleset v1 en Python (aproximación, no el motor) con 10 000 peleas: win rate marginal por stat hp 64 %, defense 58 %, attack 39 %, speed 39 %, special 33 %; roster de 4 arquetipos 12/32/92/63 %; duración p50 45 s (49 % < 45 s); momentos 6,2 %. Con los cambios 1–4: stats entre 44 y 50 %, 84 % de las peleas en 45–92 s, momentos 17,3 %.
- Cambios pedidos:
  1. **Defensa contra especiales:** para `special`, `base = power·(100 + special_atk)/(100 + special_def)`; `defense` solo para `strike` y `heavy`.
  2. **Constantes re-centradas:** `max_hp = 500 + 4·hp`; `block‰ = 60 + defense/2 + caution`; `dodge‰ = 40 + 2·speed`; `interval = 1800 − 10·speed`.
  3. **Desesperación:** con HP propio ≤ 250 ‰ al resolver, crítico +150 ‰ y daño ×5/4. Orden entero: varianza → desesperación → crítico → debilidad → bloqueo.
  4. **Momentos alcanzables:** `comeback` con umbral 250 ‰; redefinir `sudden_ko` (≈ 0 % tal cual), p. ej. KO "al borde" con el ganador a ≤ N ‰ de HP (N = 60 ≈ 20 %), ajustando `MomentKind` antes de fusionar la fase A; definir el `side` de `decisive_crit` (el atacante) y `d_sin_crit` (con desesperación y debilidad, sin el ×3/2).
  5. **Ajustable en fase B:** enumerar como campos de `Ruleset` todas las constantes (tabla, umbrales de momentos y de desesperación, `intro_ms`, `time_limit_ms`, `end_ms`); un cambio de forma de fórmula enmienda la spec.
  6. **Ritmo con 002:** `end_ms = 4000`; `time_limit_ms = 86 000`; banda de RF17 45 000–90 000 ms.
  7. **Recuperación explícita:** `next_at_def = max(…, t_res + 300)` en golpe, bloqueo y esquiva.
  8. **Medición:** el win rate global excluye espejos; definir `P` en `m = ⌊N/(2·P)⌋` y si los espejos cuentan en N; sesgo del lado `a` agregado sobre todos los espejos; reportar el % de peleas ganadas tras ir ≥ 250 ‰ abajo y advertir (sin fallar) cruces fuera de 30–70 %.
  9. **Coordinación con 002:** fin por tiempo/empate (igual que el PO); windups del motor (700/1100 ms) más largos que el ≤ 400 ms de las animaciones: el adaptador decide qué se ve.
  10. **RF14:** el skill `write-character` define dónde viven los nombres localizados de las habilidades (p. ej. `abilities: { <id>: <nombre> }` en el frontmatter de `lore.<locale>.md`).

### architect — 2026-10-26 — Respuesta a la ronda 1
- **product-owner 1 (techo de duración):** `time_limit_ms = 86 000` y `end_ms = 4000`. `end.t` es la duración total (intro y pose incluidas) y cumple `end.t ≤ 90 000`. La banda de RF17 pasa a 45 000–90 000 ms.
- **product-owner 2 y game-designer 9 (fin sin KO frente a la `Timeline` v1):**
  - Decisión: el log conserva `time_up`, `decision` y `draw`, porque es honesto y verificable, y RF17 las hace raras (≤ 10 % y < 1 %).
  - Se pide `timeline_version` 2 con dueño: la fase C de 002 (frontend-engineer), por RF-C4, con lo que debe expresar. Mientras no exista, el adaptador da `adapter.unsupported_ending`, y ninguna feature publica peleas reales sin v2 (preocupación 7).
  - Se completó la tabla de correspondencias, incluidos `crit` → `result: crit`, `dodge` y `knockdown`. Se aclaró que el windup del motor no es el que se ve.
- **product-owner, no bloqueantes:** `damage` pasa a ser el daño aplicado (`hp = anterior − damage`, invariante en RF2); `sim` avisa de los cruces fuera de 30–70 %; `clutch_ko` no es el "favorito perdiendo".
- **security-reviewer 1:**
  - TA-13 enlazado en las preocupaciones 2 y 3.
  - Nuevo RF22, requisito duro que bloquea producción: snapshot publicado o commit extendido.
  - `CharacterSpec` con `Serialize`/`Deserialize` y `deny_unknown_fields`, `canonical_spec_json`, dorado e ida y vuelta (RF16 c). El WASM acepta solo la forma motor.
- **security-reviewer 2:** se agregan los lints `deny` pedidos en `engine` y `engine-wasm`; `roll(NonZeroU32)` calcula en `u64`; `canonical_json` y `canonical_spec_json` devuelven `Result<_, EncodeError>`.
- **security-reviewer 3:**
  - `clippy.toml` con los `disallowed-methods`/`types` pedidos.
  - `scripts/check-engine.mjs` revisa el grafo y las features (`serde`, `os_rng` y `std` de `rand_chacha`/`rand_core`) y tiene tests negativos.
  - Se documenta la resolución de features.
- **security-reviewer 4:** `deny` con un único `allow(unsafe_code)` en la función exportada. El script falla si aparece `\bunsafe\b`, si hay más de un `allow` o si la tabla `[lints]` difiere en algo más que `unsafe_code`.
- **security-reviewer, recomendaciones tomadas:**
  - targets wasm en `deny.toml`;
  - test exacto del `Debug` de `Seed`, que además no implementa `Display`/`LowerHex`/`Serialize` ni expone sus bytes;
  - lista de códigos de RF19, límite de 8 KiB antes de parsear y sin mensajes de `serde_json`;
  - runner sin `preopens` y con `env: {}`;
  - reembolso de `draw` con asientos del ledger (preocupación 9).
- **game-designer 1 a 3:** se adoptaron tal cual (defensa especial, constantes re-centradas, desesperación con el orden varianza → desesperación → crítico → debilidad, y bloqueo sin desesperación). Lo verifiqué con su `sim.py`, parcheado: stats entre 44 y 52 %, 92,8 % de las peleas en 45–90 s, momentos en el 32,7 %, 100 % de KO, separación mínima entre impactos de 700 ms y 50,2 % del lado `a` en los espejos.
- **game-designer 4:** `comeback` pasa a 250 ‰. `sudden_ko` se reemplaza por `clutch_ko` (el ganador a ≤ 60 ‰, ~20 % de las peleas en la aproximación). Se definen el `side` de `decisive_crit` (el atacante) y `d_sin_crit`.
- **game-designer 5:** todas las constantes son campos de `Ruleset`, enumerados en la tabla. Cambiar la forma de una fórmula enmienda la spec.
- **game-designer 6 y 7:** adoptados (86 000 / 4000 / 45–90 s, y recuperación explícita en golpe, bloqueo y esquiva).
- **game-designer 8:** se definen `P = C(n, 2)`; los espejos quedan fuera de `N` y del win rate; el sesgo del lado `a` se agrega sobre todos los espejos; se reporta el % de victorias tras ir ≥ 250 ‰ abajo.
- **game-designer 10:** RF14 fija `abilities: { <id>: <nombre> }` en el frontmatter de `lore.<locale>.md`.
- **Datos de 002 sin pedido de un revisor:** el `knockdown` lo emite el motor (1500 ms en un crítico que aplica ≥ 100 ‰ del `max_hp` y no noquea, sin golpes durante la caída). Las reglas de ritmo de 002 son invariantes de RF2, y `Ruleset` las comprueba con un test. `taunt` queda como extensión futura.

### product-owner — 2026-10-26 — APPROVE (ronda 2)
- Verifiqué: cambio 1 resuelto (`time_limit_ms = 86 000`, `end_ms = 4000`, `end.t ≤ 90 000` como duración total; en KO quedan 2,5 s de pose, ≥ 2 s de 002); cambio 2 resuelto (tabla de correspondencias completa, decisión de conservar `time_up`/`decision`/`draw`, dueño de `timeline_version` 2, `adapter.unsupported_ending` y puerta de no publicar peleas reales sin v2; RF17 las limita a ≤ 10 % y < 1 %); no bloqueantes tomados (`damage` aplicado, aviso de cruces 30–70 %, `clutch_ko` separado del "favorito perdiendo"); lo cambiado por otras revisiones no mete scope creep; criterios del intent y ADRs 0004/0005/0007 siguen cubiertos.
- No bloqueante: (1) dueño de la v2 como "la fase C de 002 o la feature que herede RF-C1 a RF-C4"; (2) registrar la puerta "sin Timeline v2 no se publican peleas reales" fuera de esta spec (prerrequisitos en `docs/architecture.md` o Linear); (3) confirmar en la fase B con el motor real que la ruta `time_up` sigue cubierta por los dorados de RF16 (a).

### security-reviewer — 2026-10-26 — APPROVE (ronda 2)
- Verifiqué: cambio 1 (RF22 duro con dueño en TA-13, enlazado en las preocupaciones 2 y 3; `CharacterSpec` canónico con `deny_unknown_fields`, dorado e ida y vuelta; el WASM acepta solo la forma motor; la concatenación del commit extendido no es ambigua); cambio 2 (lints de RF5, `roll(NonZeroU32)` en `u64`, `Result<_, EncodeError>` → `log.encode_failed`; cotas recalculadas: ≤ 68 acciones por lado, sin divisiones por cero, `damage` cabe en `u16`); cambio 3 (`clippy.toml`, `scripts/check-engine.mjs` con tests negativos; comprobé que `cargo tree -p engine` resuelve sin falsos positivos por `tungstenite`); cambio 4 (`deny` + un único `allow`, chequeo de `\bunsafe\b` y de la tabla `[lints]`); recomendaciones de la ronda 1 adoptadas; knockdown, desesperación y `clutch_ko` no consumen PRNG ni exponen la semilla.
- Recomendaciones para el plan/PR de la fase A: definir qué hace el motor si se alcanza `max_events` (ni truncar en silencio ni pánico) y añadir la cota analítica al test de `Ruleset::v1()`; reformular RF21 ("como mucho 3 eventos por acción" no es literal; la cota ≈ 438 < 512 se sostiene); en `clippy.toml` usar rutas completas y `allow-invalid = true` si clippy no resuelve `from_os_rng`; para TA-13, fijar la codificación exacta del commit extendido y recalcular `canonical_spec_json` en el verificador.

### game-designer — 2026-10-26 — APPROVE (ronda 2)
- Verifiqué: los 10 cambios uno por uno; corrí de nuevo `arch/roster2.py` y `arch/marg2.py` (aproximación en Python): win rate marginal por stat 44–52 %, 92,8 % de las peleas en 45–90 s (p10/p50/p90 = 46/57/70 s), momentos 32,7 % (clutch_ko 20,4, decisive_crit 10,8, comeback 9,1), 100 % KO, ≥ 700 ms entre impactos, lado `a` 50,2 % en espejos. El balance por personaje (28–75 %) es de contenido y se ajusta en la fase B. `knockdown` cumple los datos de 002 y no consume PRNG; `clutch_ko` cubre la "sorpresa" del intent; el ejemplo JSON cuadra.
- No bloqueante: nombrar como campo de `Ruleset` el "×2 si está desesperado" del peso de `special` (p. ej. `w_special_desperation_mul`); en la fase B, que ningún `kind` de momento quede muerto (p. ej. cada uno ≥ 3 %).

### owner — 2026-10-26 — APPROVE
- OK explícito en chat (gate de `risk: high`, `docs/process.md`).

### architect — 2026-10-26 — Enmienda editorial (revisión de plan-a)
- Sin cambio de comportamiento ni de forma; no reabre las aprobaciones. Origen: decisiones 1, 3 y 7 de `plan-a.md` y la recomendación de security-reviewer (ronda 2) sobre RF21 y `max_events`.
- **RF21:** la cota pasa a ser la exacta (68 acciones por lado, 3 eventos por acción y 10 eventos únicos: 418 < 512) y se nombra la guarda `SpecError::EventLimit`.
- **RF4:** `event_limit` entra en la lista de ejemplo de `SpecError`, y "la simulación no falla" queda acotado a los rulesets que pasan el test de cotas.
- **RF2:** `hp = 0` aparece en la resolución (`hit` o `block`) que precede al `ko`. Lo implicaba RF9 (el daño de bloqueo también se aplica), y ahora el texto lo dice para que `assert_invariants` no lo interprete como "solo `hit`".
- **Modelo de datos:** `FightLog::outcome()` devuelve `Option<…>`, porque un log deserializado puede no tener `end` y `protocol` no puede entrar en pánico.

### architect — 2026-10-27 — Enmienda menor (revisión del PR #39), pendiente de security-reviewer
- **Cambio de mecanismo en RF19 y la preocupación 4.** No cambia el comportamiento del motor ni el formato. Origen: la desviación 9 de `plan-a.md` y el Important de security-reviewer en el PR #39: `check-engine` se podía esquivar con `cfg_attr`, con macros, con `#[path]` o con `include!`.
  - `engine-wasm` pasa a `[lints] workspace = true` (`forbid`) y no lleva ningún `allow(unsafe_code)`. Lo comprobé en una copia del árbol: con `forbid` y sin el `allow`, clippy pasa para `wasm32-unknown-unknown` con `-D warnings`, el `.wasm` de release exporta `simulate` y cualquier `allow(unsafe_code)` da `E0453`.
  - `scripts/check-engine.mjs` exige cero atributos que relajen `unsafe_code` (directos, de crate, en `cfg_attr` o en macros), rechaza `include!` y `#[path` en `src/`, exige `[lints] workspace = true` y conserva la búsqueda del token `unsafe`.
  - Se ajustan los casos negativos de "Estrategia de pruebas" (Scripts).
- Las aprobaciones de las demás áreas siguen en pie. **La enmienda queda pendiente de la aprobación de security-reviewer**, que pidió el diseño anterior (`deny` y un `allow`). La implementación no se fusiona hasta que llegue esa aprobación. Yo la escribí, así que no la apruebo.

### security-reviewer — 2026-10-27 — APPROVE (enmienda de RF19 y la preocupación 4)
- Apruebo el cambio a `forbid` sin ningún `allow` en `engine-wasm`. Es más fuerte que el diseño `deny` con un `allow` que pedí en la ronda 1, porque el control lo aplica rustc y no un script.
- Verifiqué en `168cf26`:
  - clippy para `wasm32-unknown-unknown` pasa con `-D warnings`;
  - en un crate aparte con `unsafe_code = "forbid"`, un `allow(unsafe_code)` da `E0453`, también si lo genera una macro o viene de un archivo con `include!`, y un bloque `unsafe` incluido no compila;
  - las cuatro variantes del PR #39 (`cfg_attr`, macro, `#[path]`, `include!`) ahora hacen fallar `check-engine`;
  - `checkWasmLints` exige `[lints] workspace = true`, que es la pieza que sostiene el `forbid`.
- `check-engine` todavía no detecta todas las formas, por ejemplo `use core::include as inc;` o pasar `unsafe_code` dentro de un `deny(...)` a una macro. Como el compilador ya las rechaza con `forbid`, queda como defensa en profundidad y no bloquea.

### architect — 2026-10-27 — Enmienda (revisión de plan-b), en parte pendiente de game-designer
- Origen: las preguntas de `plan-b.md` (decisiones 1, 2, 6, 8, 10, 13 y 16) y la nota no bloqueante del game-designer en la ronda 2 ("ningún `kind` de momento muerto").
- **Editorial, sin cambio de comportamiento (no reabre aprobaciones):**
  - RF12: firma `load_roster(root, ruleset) -> Result<Vec<Character>, ContentError>`, con `archetype` y `rivals`, y `is_valid_slug` exportado por el motor.
  - RF15: los espejos solo cuentan para el sesgo del lado `a`; el resto de las métricas y de RF17 son sobre los cruces (lo implicaba "no cuentan en `N`").
  - Componentes: dependencias de `sim` (`serde`, `thiserror` y `rand_chacha` sin features, todas ya en el lock).
  - RF5: el `clippy.toml` del motor suma la prohibición de I/O, entorno, hilos y procesos (defensa en profundidad de lo que RF5 ya exige).
- **Cambio de criterio, pendiente de la aprobación de game-designer:**
  - RF17 suma "cada `MomentKind` en al menos el 3 % de las peleas" como criterio que falla. La aproximación en Python da 9,1 / 10,8 / 20,4 %, así que el margen es amplio.
  - RF17 precisa qué umbrales puede ajustar el game-designer: la fracción del 80 %, el tope de `time_up`, la banda del sesgo y el 3 %. La banda de 45–90 s es del intent y de 002 y no se ajusta. Subir `time_up` por encima del 10 % necesita además el OK del product-owner, que aprobó la spec contando con ese tope (preocupación 7).
  - Si game-designer no lo aprueba, el criterio del 3 % queda como aviso (`warnings()`), como prevé plan-b, y se revierte este punto.
- Al cerrar la fase B haré las enmiendas editoriales que dependan de valores medidos: la tabla del ruleset y el párrafo de la aproximación en Python (con las cifras reales de `sim`), las cifras de RF21 (68 acciones, 1200 ms, 418 eventos) y de la preocupación 8 si cambian los tiempos, y los umbrales de RF17 que el game-designer haya ajustado. Yo escribí esta enmienda, así que no la apruebo.

### game-designer — 2026-10-27 — APPROVE (enmienda de revisión de plan-b)
- Verifiqué:
  - RF17, "cada `MomentKind` ≥ 3 %" como fallo: es la nota no bloqueante de mi ronda 2 y la acepto tal cual. El universo es el de RF15 (cruces, con al menos un `moment` de ese `kind`), igual que plan-b (decisión 10 y revisión de architect, punto 1). Con unos 10 000 cruces, el 3 % son unas 300 peleas, así que el ruido de muestreo no lo vuelve frágil, y la aproximación (comeback 9,1, decisive_crit 10,8, clutch_ko 20,4 %) deja margen de unas 3 veces en el `kind` más raro. No vuelve redundante el 15 % de "algún `moment`": tres `kind` en el 3 % pueden sumar solo un 9 %.
  - Umbrales ajustables: la fracción del 80 %, el tope de `time_up`, la banda del sesgo y el 3 %. Estoy de acuerdo en que la banda de 45–90 s no se toque (viene del intent y del ritmo de 002) y en que subir `time_up` por encima del 10 % pida el OK del product-owner. El 400–600 ‰ de win rate, el 15 % de momentos y el `draw` < 1 % quedan fijos, y está bien así: son el objetivo de balance de mi rol.
  - RF5, RF12, RF15 y la tabla de componentes: son editoriales y no cambian gameplay. "Solo cruces" para todas las métricas evita que un personaje fuerte en espejo distorsione la duración o los momentos, y `archetype` y `rivals` en `Character` alcanzan para el narrador.
  - Frontmatter válido. No contradice ADR-0001, 0004, 0005 ni 0010.
- Compromiso propio, no bloqueante: si en la fase B un `kind` no llega al 3 % con contenido, no voy a bajar el 3 %. Primero ajusto valores de `Ruleset` (decisión 14 de plan-b). Si tampoco alcanza, lo escalo como cambio de forma de la fórmula del momento. Bajar el piso equivaldría a aceptar un momento muerto. Si ajusto cualquier otro umbral, lo registro con su cifra medida en esta sección y en el PR.
- Cambios pedidos: ninguno. Actualicé mi fecha en `approvals`. `status` sigue `approved`.

### architect — 2026-10-27 — Enmienda editorial (revisión del PR #43)
- Reemplacé las cifras de la aproximación en Python por las de `sim` (`just sim` en release, ruleset v1, semilla raíz por defecto, 9996 cruces y 3332 espejos, informe en el cuerpo del PR #43): el párrafo de la tabla del ruleset, la preocupación 7 (0 % de `time_up` y de `draw` con el contenido real) y la preocupación 8 (36/43/51 resoluciones por pelea en p10/p50/p90 y separación media de ~1096 ms, frente a los ~1,9 s de 002).
- Las cifras de la aproximación que quedan en las rondas anteriores de esta Revisión se dejan como registro histórico.
- No cambia ningún requisito, fórmula ni umbral de RF17: RF17 pasa con los valores iniciales. Las cifras de RF21 (68 acciones, 1200 ms, 418 eventos) no cambian, porque `interval_*` y los windups siguen igual.
- Es editorial y la escribí yo, así que no la apruebo. `status` sigue `approved`.
