---
id: "003"
title: Motor de pelea v0 — fase B
type: plan
status: approved
risk: high
author: backend-engineer
reviewers: [architect]
approvals:
  - {by: architect, date: 2026-10-27}
linear: TA-3
depends_on: ["001"]
created: 2026-10-27
updated: 2026-10-27
---

# Plan: Motor de pelea v0 — fase B (desde spec.md 2026-10-27)

> [intent](./intent.md) · [spec](./spec.md) · [plan-a](./plan-a.md) · ADRs [0001](../../decisions/0001-motor-propio-determinista.md), [0002](../../decisions/0002-commit-reveal-semilla.md), [0004](../../decisions/0004-personajes-homenaje.md), [0007](../../decisions/0007-i18n-desde-cero.md)

Alcance de la fase B: `crates/content` (RF12, RF13), de 2 a 4 personajes homenaje con el skill `write-character` actualizado (RF14), `crates/sim` con su CLI y `just sim` (RF15), el test de balance sobre el contenido real (RF17), la medición del RNF de rendimiento y las notas no bloqueantes de la revisión de la spec que se dejaron para esta fase. La fase A (`plan-a.md`, PR #39) ya está en `main`: este plan no cambia el formato del log ni las fórmulas.

## Supuestos verificados (2026-10-27, solo lectura)
- **Lock:** en una copia del workspace en el scratchpad agregué `crates/content` (deps `engine`, `serde`, `serde_json`, `thiserror`) y `crates/sim` (deps `engine`, `content`, `serde`, `serde_json`, `thiserror`, `rand_chacha`) como miembros y corrí `cargo metadata --offline`. El diff de `Cargo.lock` son **solo** los dos `[[package]]` de ruta (`content` y `sim`), sin ninguna versión de registro nueva. `anyhow`, `clap` y similares **no** están en el lock, así que no se usan.
- **Grafo de los consumidores:** `cargo tree -p content -p sim -e normal,features --target all --prefix none` no contiene `getrandom`, `rand`, `tokio`, `ts-rs`, `tracing` ni `log`, y `rand_chacha`/`rand_core` no tienen `serde`, `os_rng` ni `std` (de `rand_core` solo `feature "default"`). `parseCargoTree` de `scripts/check-engine.mjs` acepta esa salida tal cual (líneas con `(*)` y `(command-line)` incluidas).
- **CI:** el filtro `rust` de `ci.yml` ya incluye `crates/**` y `content/**`, así que un PR que solo toque personajes corre el test de balance. `content/` y `**/*.md` están en `.prettierignore`, así que el filtro `web` no necesita `content/**`.
- **Consumidores de los dorados:** ningún archivo de `apps/` ni `packages/` lee todavía `crates/engine/tests/golden/` (la fase C de 002 no empezó). Regenerarlos en esta fase no rompe a nadie.
- **Imagen del server:** `apps/server/Dockerfile` copia `crates/` entero y compila solo `-p server`, así que los dos miembros nuevos no rompen `cargo chef` y no entran en el binario.
- **API del motor que hace falta:** `Ruleset::for_version`, sus getters (`comeback_deficit_permille`, `time_limit_ms`…), `CharacterSpec::validate`, `simulate`, `canonical_json` y `Seed::from_bytes` son públicos. El patrón de `slug` (`is_code`) es privado de `spec.rs` (ver la decisión 2).

## Decisiones de este plan
1. **Firma del cargador.** `content::load_roster(root: &Path, ruleset: &Ruleset) -> Result<Vec<Character>, ContentError>`, con `pub struct Character { pub spec: CharacterSpec, pub archetype: String, pub rivals: Vec<String> }`, ordenado por `spec.slug`. El `ruleset` es necesario porque `ruleset_version` "es igual a la versión con que se carga" y `validate` lo recibe. `archetype` y `rivals` no se descartan: el narrador y el server los usarán, y descartarlos ahora obligaría a reabrir el cargador. `sim` usa solo `spec`. Es una precisión de la firma `load_roster(root)` de RF12.
2. **El patrón de `slug` sale del motor.** `engine` exporta `pub fn is_valid_slug(value: &str) -> bool` (hoy `is_code(value, b'-')`), y `validate` pasa a llamar a `is_valid_slug` para el `slug`, así que hay una sola definición; su doc dice que es una regla de formato, no de juego. `content` lo usa para `archetype`, que no está en la forma motor. Así las expresiones regulares siguen viviendo solo en el motor (RF12). Es una función nueva y pura: no cambia ningún byte de los dorados.
3. **`ContentError` (thiserror), sin texto de `serde_json`.** Variantes con `code()` estable en `snake_case`: `io` (lectura fallida), `name_not_utf8`, `unexpected_entry` (symlink u otro tipo que no es archivo ni directorio), `missing_character_file`, `file_too_large` (más de 64 KiB), `invalid_json` (solo `line` y `column`, nunca el mensaje de serde), `slug_mismatch`, `archetype_invalid`, `ruleset_version_mismatch`, `missing_lore` (con el `locale`), `spec` (envuelve el `SpecError` del motor), `rival_unknown`, `rival_self` y `rival_duplicate`. Cada error lleva la ruta relativa a `root` y, cuando se conoce, el `slug`: es contenido del repo, no un dato de usuario, y sirve para corregir el archivo. Si el server lo registra más adelante, cumple ADR-0012 registrando `code()` y la ruta.
4. **Recorrido del directorio.** En `root`, los archivos regulares se ignoran (`README.md`), cada directorio es un personaje y cualquier otra entrada (symlink incluido) es `unexpected_entry`: no se sigue un enlace fuera de `content/`. El nombre del directorio debe ser UTF-8. `character.json` se lee con un tope de 64 KiB (`file_too_large`) antes de parsear. El resultado se ordena por `slug` porque `read_dir` no garantiza orden. Un directorio sin personajes devuelve `Ok(vec![])`: el mínimo de 2 lo exigen `sim` y el test de RF13.
5. **Lore: solo existencia.** `LOCALES = ["en", "es"]` (ADR-0007) en `content`, y se exige que `lore.<locale>.md` sea un archivo regular. El frontmatter (`name`, `epithet`, `locale` y `abilities: { <id>: <nombre> }`) lo fija el skill `write-character`, y su parseo y validación son de la feature que lo muestre (RNF de i18n). Riesgo de deriva entre `abilities` del lore y los `id` del JSON: ver Riesgos.
6. **Dependencias de `sim`:** `engine`, `content`, `serde` (derive de `Serialize` para `--json`), `serde_json`, `thiserror` y `rand_chacha` (`ChaCha20Rng` para derivar las semillas, `workspace = true`, sin features). Todas están en el lock. La tabla de componentes de la spec nombra solo `engine`, `content` y `serde_json`; es una precisión, no un cambio de diseño. El bin no usa `anyhow` (no está en el lock): `main` devuelve `ExitCode` y los errores son `SimError` (thiserror).
7. **Derivación de semillas.** `ChaCha20Rng::from_seed(root)`, y cada semilla son 8 `next_u32()` en little endian (32 bytes). Orden fijo: primero los cruces, recorriendo los pares `{i < j}` en orden de `slug` y, dentro de cada par, `k = 0..m`; cada semilla se juega como `(i, j)` y como `(j, i)`. Después los espejos, `i` en orden de `slug` y `k = 0..m`, con semillas nuevas del mismo generador. Cambiar este orden cambia el informe (no los dorados del motor). `n < 2` es `SimError::RosterTooSmall`, y `m = 0` (`N < 2·P`) es `SimError::TooFewFights`. `--fights` tiene un tope de 10 000 000.
8. **Universo de cada métrica.** Todas las métricas (win rates, matriz, % de KO/decisión/empate, duración, momentos, knockdown, densidad y victorias tras ir ≥ 250 ‰ abajo) se calculan **solo sobre los cruces** (las `2·P·m` peleas de `N`). Los espejos se usan **solo** para el sesgo del lado `a`. RF15 dice que los espejos no cuentan en `N`; extenderlo a todas las métricas evita que un personaje dominante en espejo distorsione la duración o los momentos.
9. **Aritmética entera y comparaciones exactas.** El informe guarda conteos. Las tasas con empates usan "medios puntos" (`2·victorias + empates` sobre `2·apariciones`) y los criterios se comparan por multiplicación cruzada (por ejemplo, `800·apariciones ≤ 1000·(2·victorias + empates) ≤ 1200·apariciones`), sin floats ni redondeos en el borde. El texto muestra ‰ con división entera. Los percentiles son de rango más cercano sobre los `end.t` ordenados. La victoria "tras ir ≥ 250 ‰ abajo" se reconstruye del log: se sigue el `hp` de cada lado por los `hit`/`block`, con `hp·1000/max_hp` de `FighterInfo`, y el umbral es `ruleset.comeback_deficit_permille()` (no un literal).
10. **Notas no bloqueantes de la revisión de la spec:**
    - **Ningún `MomentKind` muerto (game-designer):** cada `kind` aparece en al menos el 3 % de los cruces ("% de cruces con al menos un `moment` de ese `kind`"). Es un criterio que **falla** en `--check` y en el test, porque un `kind` muerto es justo lo que el narrador no puede usar. Architect lo aprobó y enmendó RF17; queda pendiente de la aprobación de game-designer, y si no la da, pasa a aviso. Por eso vive en `Criteria::RF17`, de modo que pasar de fallo a aviso sea un cambio de una línea.
    - **Cruces fuera de 30–70 %:** `Report::warnings()`, que se imprime y nunca hace fallar `--check` ni el test.
    - **Sesgo del lado `a` en espejos:** `(2·victorias_a + empates)` sobre `2·espejos`, criterio 47–53 % de RF17.
    - **% de victorias tras ir ≥ 250 ‰ abajo:** se reporta, sin criterio.
    - **`w_special_desperation_mul`:** ya es campo de `Ruleset` (decisión 5 de plan-a). Si el game-designer lo ajusta, sigue el protocolo de la decisión 14.
    - **La ruta `time_up` sigue cubierta (product-owner):** ver la decisión 15.
11. **Densidad para 002 (preocupación 8).** El informe agrega, solo informativo, las resoluciones por pelea (p10/p50/p90) y la separación media entre resoluciones consecutivas, para comprobar los "≥ 30 golpes, ~1,9 s" que pidió 002.
12. **Salida del CLI.** Código 0 si todo va bien, 1 si `--check` encuentra un criterio que falla, 2 en un error de uso, de contenido o de simulación (con el `code()` y la ruta en stderr). El informe de texto y el `--json` incluyen la semilla raíz en hex: es la semilla del simulador (por defecto, `*b"torneo-absurdo-sim-default-v1!!!"`), no la de una pelea real, y hace falta para reproducir un informe pegado en un PR. El `--help` dice que el CLI nunca recibe semillas de peleas reales. El tiempo se mide con `Instant` en `main.rs`, alrededor de `sim::run` (que incluye `canonical_json` de cada log), y se agrega al informe fuera de la lib, así que `Report` es determinista. `print_stdout`/`print_stderr` solo con `#[allow]` local en las dos funciones de `main.rs` que escriben.
13. **Pureza del motor con consumidores que sí hacen I/O.**
    - **Dirección:** `content` y `sim` dependen de `engine` y nunca al revés (cargo rechaza el ciclo). `check-engine` sigue mirando el grafo de `engine` **solo** (`cargo tree -p engine`), que es lo que compilan `engine-wasm` y el target wasm, y eso no cambia con los crates nuevos.
    - **Unificación de features:** `check-engine.mjs` aplica además `checkEngineGraph` (sin exigir la raíz `engine`, que aparece como dependencia) a `cargo tree -p content -p sim -e normal,features --target all`. Así un `features = ["std"]` o `["serde"]` en `rand_chacha` desde `sim`, o un `rand`/`getrandom` en los consumidores, falla aunque el grafo de `engine` solo siga limpio. Lo verifiqué sobre la copia del scratchpad (ver Supuestos).
    - **Sin I/O en el código del motor (defensa en profundidad, aprobada por architect):** `crates/engine/clippy.toml` suma en `disallowed-types` `std::fs::File`, `std::fs::OpenOptions`, `std::process::Command`, `std::net::{TcpStream, TcpListener, UdpSocket}` y `std::thread::Builder` (un tipo prohibido cubre todos sus métodos), y en `disallowed-methods` las funciones libres `std::fs::{read, read_to_string, read_dir, write, metadata, create_dir_all, remove_file, copy, rename}`, `std::env::{var, var_os, vars, vars_os, args, args_os, current_dir}`, `std::thread::spawn` y `std::io::{stdin, stdout, stderr}`. Clippy resuelve rutas, así que un `use std::fs as f` no lo esquiva. El único uso legítimo hoy es `check_golden` de `tests/golden.rs` (`std::fs::write` y `std::env::var_os` para `UPDATE_GOLDEN`), que lleva un único `#[allow(clippy::disallowed_methods, reason = "…")]` local (más `clippy::disallowed_types` en el mismo atributo si hace falta). Es `allow` y no `expect`: en wasm el bloque queda fuera por `cfg` y un `expect` sin cumplir avisaría. Hoy el "sin I/O" de RF5 depende de la revisión; con `content` haciendo I/O al lado, vale la pena mecanizarlo.
    - **`_check-ts-rs-graph`** extiende el bucle a `content` y `sim` (ni `ts-rs` ni `tokio`): `content` lo usará el server y debe seguir ligero.
14. **Protocolo para ajustar valores de `Ruleset` (RF10, spec "Fase B").** Primero se ajusta el **contenido**; solo si un criterio global (duración, `time_up`, momentos por `kind`, densidad) no se alcanza con contenido, el game-designer propone valores nuevos de `Ruleset::v1()`. Así:
    1. Se experimenta editando `v1()` en el árbol de trabajo y corriendo `just sim --check` (release). No se agrega un constructor de experimentos: `Ruleset` solo se construye con `for_version` (RF4).
    2. Los valores finales entran en **un commit propio** (`feat(engine): tune ruleset v1 values (TA-3)`), que contiene `ruleset.rs`, los tests unitarios que fijan valores de la tabla (`sim.rs`, `ruleset.rs`, incluido el `assert_eq!(max_events_bound(), 418)` si cambian tiempos), las semillas de `VECTORS` y todos los dorados regenerados.
    3. Regeneración: `cargo test -p engine --test golden -- --ignored --nocapture search_golden_seeds` si algún vector deja de cubrir lo suyo (el test `vectors_match_their_golden_logs` lo detecta con `covers`); se fijan los índices nuevos en `VECTORS`; después `UPDATE_GOLDEN=1 cargo test -p engine --test golden` en nativo; se revisan a mano un log de KO y uno de decisión contra las fórmulas; y `just _check-wasm` tiene que pasar con los dorados nuevos en WASI. `spec-*.json` no cambia salvo que cambien los rangos de validación, que no se tocan.
    4. Las cotas del test de `Ruleset::v1()` (ritmo de 002 y la cota de eventos < 512) y las invariantes de RF2 de `properties.rs` siguen pasando sin tocarlas: son el límite del ajuste.
    5. Si la búsqueda no encuentra un empate (20 000 semillas por par de fixtures), se quita `log-draw.json` y RF9 queda cubierto por su test unitario (lo permite RF16 a); se registra como desviación.
    6. Los valores de la tabla del ruleset y los números de RF21 (68 acciones, 418 eventos) de la spec los actualiza architect como enmienda editorial: yo no edito la spec. El PR declara que `ruleset_version = 1` sigue en preproducción (no hay ninguna pelea guardada en producción) y lo avisa a 002 junto con el pedido de la `timeline_version` 2.
15. **Cobertura de `time_up` (product-owner, no bloqueante 3).** Los dorados siguen en los fixtures de `crates/engine/tests/fixtures/` (RF16 los desacopla de `content/` a propósito). Tras cualquier regeneración, `vectors_cover_every_event_and_moment` exige `time_up` y `decision`, y los `covers` de `log-decision-a/b.json` exigen una decisión de cada lado, así que la ruta no puede perderse en silencio. Con el contenido real, `time_up` será raro (≤ 10 %, RF17); `sim` reporta su porcentaje y el PR lo pega. No agrego un dorado sobre `content/`, porque acoplaría el balance a los dorados, que es lo que RF16 evita.
16. **Umbrales de RF17 en un solo lugar.** `sim::Criteria::RF17` (constante) los guarda todos, y lo usan `--check` y el test. El game-designer solo puede ajustar (RF17 enmendado): la fracción del 80 % de peleas dentro de la banda de duración, el tope de `time_up` (subirlo por encima del 10 % necesita además el OK del product-owner), la banda del sesgo del lado `a` en espejos y el 3 % por `MomentKind`. La banda de 45 000–90 000 ms **no** se ajusta. Un ajuste cambia esa constante y se registra como desviación, con la enmienda editorial de la spec a cargo de architect.
17. **Cantidad de personajes.** La decide el game-designer, entre 2 y 4. Recomiendo 4: es el roster con que se calibró la aproximación en Python, da 6 cruces para la matriz y hace significativo el criterio por personaje (con 2, el win rate global es el único cruce).

## Archivos que cambian
| Archivo | Cambio |
|---|---|
| `Cargo.toml` | modificado: `members` suma `crates/content` y `crates/sim` (orden alfabético); `[workspace.dependencies]` suma `content = { path = "crates/content" }`. Solo si hace falta (Riesgos): `[profile.dev.package.engine] opt-level = 2`. |
| `Cargo.lock` | modificado: solo los `[[package]]` de `content` y `sim` (verificado). |
| `scripts/check-crate-age.mjs` | modificado: `WORKSPACE_MEMBERS = ['content', 'engine', 'engine-wasm', 'protocol', 'server', 'sim']` (su test exige que coincida con `[workspace].members`). |
| `crates/engine/src/spec.rs`, `crates/engine/src/lib.rs` | modificado: `pub fn is_valid_slug(&str) -> bool`, que `validate` usa para el `slug`, y su reexport; test unitario que comparte los casos de `slug_pattern` (decisión 2). |
| `crates/engine/clippy.toml` | modificado: tipos y funciones de I/O prohibidos (decisión 13). |
| `crates/engine/tests/golden.rs` | modificado: un único `#[allow(clippy::disallowed_methods, reason = "…")]` local en `check_golden` (decisión 13). Si se ajusta el ruleset: semillas de `VECTORS` (decisión 14). |
| `crates/engine/src/ruleset.rs`, `crates/engine/src/sim.rs` (tests), `crates/engine/tests/golden/*` | solo si se ajusta el ruleset, en un commit propio (decisión 14). |
| `crates/content/Cargo.toml` | nuevo: deps `engine`, `serde`, `serde_json`, `thiserror`; `[lints] workspace = true`. |
| `crates/content/src/lib.rs` | nuevo: `load_roster`, `Character`, `LOCALES`, `MAX_FILE_BYTES`; doc del contrato (I/O y conversión; la validación es del motor). |
| `crates/content/src/file.rs` | nuevo: `CharacterFile` (forma de archivo con `deny_unknown_fields`, campos en el orden de la spec) y `into_character(self) -> Character`. |
| `crates/content/src/error.rs` | nuevo: `ContentError` y `code()` (decisión 3). |
| `crates/content/tests/load.rs` | nuevo: integración sobre fixtures (ver Pruebas). |
| `crates/content/tests/real_content.rs` | nuevo: RF13 sobre `content/characters/`. |
| `crates/content/tests/fixtures/<caso>/<slug>/…` | nuevo: un roster por caso (válido con 3 personajes y un `README.md`, campo desconocido arriba y anidado, slug distinto del directorio, `archetype` inválido, `ruleset_version` 2, falta `lore.es.md`, falta `lore.en.md`, presupuesto excedido, JSON roto, falta `character.json`, rival desconocido, rival propio y rival duplicado). |
| `crates/sim/Cargo.toml` | nuevo: lib + `[[bin]] name = "sim"`; deps de la decisión 6; `[lints] workspace = true`. |
| `crates/sim/src/lib.rs` | nuevo: `Config`, `run(&Config, &[CharacterSpec], &Ruleset) -> Result<Report, SimError>`, la planificación de la decisión 7 y `SimError`. |
| `crates/sim/src/report.rs` | nuevo: `Report` (conteos, `Serialize`), métricas, `Criteria::RF17`, `failures()`, `warnings()` y `render_text()` (en inglés). |
| `crates/sim/src/args.rs` | nuevo: parseo de `--fights`, `--seed`, `--content`, `--json`, `--check` y `--help`, a mano, y `parse_seed_hex` (64 caracteres hex, mayúsculas o minúsculas). |
| `crates/sim/src/main.rs` | nuevo: `ExitCode`, `Instant`, impresión con `#[allow]` local (decisión 12). |
| `crates/sim/tests/balance.rs` | nuevo: RF17 sobre el contenido real, 10 000 peleas y semilla por defecto. |
| `crates/sim/tests/cli.rs` | nuevo: el binario (`env!("CARGO_BIN_EXE_sim")`) con rosters de `crates/sim/tests/fixtures/`, `--fights` chico: códigos de salida 0/1/2 y `--json` parseable. |
| `crates/sim/tests/fixtures/{balanced,lopsided}/<slug>/…` | nuevo: dos rosters en forma de archivo, uno que pasa `--check` y otro que no (un personaje con stats mínimos). |
| `content/characters/<slug>/{character.json,lore.es.md,lore.en.md}` | nuevo: de 2 a 4 personajes del game-designer (paso 7). Sin `sprites/`: el renderer usa los placeholders de 002 y el arte es de la feature 004. |
| `content/characters/README.md` | modificado: estructura (JSON y lore por locale), `just sim --check` y el enlace al skill. |
| `.claude/skills/write-character/SKILL.md` | modificado: `character.json` con enteros 0–100 y el esquema de la spec, `abilities: { <id>: <nombre> }` en el frontmatter de cada lore, `sprites/` opcional (feature 004), balance con `just sim --check` dentro de `just check`, y quién revisa el checklist de ADR-0004. |
| `justfile` | modificado: receta `sim` (abajo) y el bucle de `_check-ts-rs-graph` con `content` y `sim`. |
| `scripts/check-engine.mjs` | modificado: segundo `cargo tree` (`-p content -p sim`) con `checkEngineGraph(tree, { requireEngineRoot: false })`; comentario de cabecera. |
| `scripts/__tests__/check-engine.test.mjs`, `scripts/__fixtures__/check-engine/tree-consumers-ok.txt` | modificado/nuevo: pasa con la salida real de los consumidores y falla con `rand_chacha feature "std"` o `getrandom` en ella. |
| `.prettierignore` | modificado: `crates/content/tests/fixtures/` (tiene un JSON roto a propósito) y `crates/sim/tests/fixtures/`. |
| `docs/architecture.md` | modificado: `content` y `sim` pasan de "la fase B agrega" a presente, con `load_roster`, la dirección `protocol ← engine ← {content, engine-wasm, sim}` y `just sim`. |
| `docs/features/003-motor-de-pelea/plan-b.md` | este plan; "Desviaciones" si la implementación se aparta. |

Sin cambios: `.github/workflows/ci.yml` (los filtros ya cubren `crates/**`, `content/**` y `scripts/check-engine.mjs`), `deny.toml`, `rust-toolchain.toml`, `crates/protocol/**` y `packages/protocol-ts/**` (el informe de `sim` es una herramienta de desarrollo y no cruza al cliente, así que no va en `protocol` ni pasa por `just gen`), `crates/engine-wasm/**`, `apps/**`. `CLAUDE.md` lo cambia solo el owner: en el PR propongo la línea de `just sim` para "Comandos".

### Receta nueva (texto exacto a implementar)
```just
# Never pass the seed of a real fight. Defaults: 10000 fights, fixed seed, content/characters.
# Mass fight simulation and balance report (003 RF15): just sim [--fights N] [--seed HEX] [--json] [--check]
[positional-arguments]
sim *args:
    cargo run -p sim --release --locked -- "$@"
```
Extensión de `_check-ts-rs-graph`: `for crate in engine engine-wasm content sim; do`, con el mismo mensaje.

## Orden de trabajo
Un commit por paso (`feat(content): … (TA-3)`, `feat(sim): …`, `feat(engine): …`, `build: …`, `docs(003): …`), en la rama `ta-3-motor-de-pelea-fase-b`. El Postgres de desarrollo es compartido con otras sesiones: nunca `just db-reset` ni `just db-down`; mientras se itera se corre `cargo test -p content -p sim -p engine --locked`, que no usa la base, y `just check` (con `db-up` idempotente) solo al cerrar cada paso.

1. **Workspace y lock.** `Cargo.toml` y los dos crates vacíos (`Cargo.toml` y `lib.rs`); `cargo metadata --format-version 1 >/dev/null` una vez, y `git diff Cargo.lock` tiene que mostrar solo `content` y `sim` (si aparece un paquete de registro, se para). `just crate-age` antes del primer build, `WORKSPACE_MEMBERS` y `just deny`. Desde aquí, todo con `--locked`.
2. **Motor: `is_valid_slug`** (con `validate` llamándolo) y el `clippy.toml` de I/O de la decisión 13 con el `allow` local en `golden.rs`. Verificación manual con un módulo temporal no commiteado que usa cada tipo y llama cada función prohibida: `cargo clippy -p engine --all-targets` tiene que marcar cada uno. Ningún dorado cambia (`cargo test -p engine`).
3. **`content`** (después de 2): `file.rs`, `error.rs`, `lib.rs` y `tests/load.rs` con sus fixtures. `real_content.rs` entra en el paso 7, cuando hay personajes.
4. **`check-engine` y `_check-ts-rs-graph`** (paralelizable con 3 y 5): segundo árbol, fixture y tests en Vitest, y el bucle de la receta.
5. **`sim`** (después de 3): `lib.rs`, `report.rs`, `args.rs`, `main.rs`, `tests/cli.rs` y sus fixtures, y la receta `just sim`. Se prueba con `just sim --content crates/sim/tests/fixtures/balanced`.
6. **Skill `write-character` y `content/characters/README.md`** (paralelizable con 3 a 5; tiene que estar antes del 7). Fija el formato que va a usar el game-designer: el JSON de la spec, el frontmatter de los lore con `abilities`, el checklist de ADR-0004 y el flujo de balance.
7. **Personajes (game-designer).** Lo invoca el orquestador (no yo), con el skill actualizado, la tabla del ruleset y el informe de `just sim` como herramientas. El game-designer:
   - diseña de 2 a 4 personajes originales (decisión 17), con `character.json`, `lore.es.md` y `lore.en.md` (humor adaptado por idioma, no traducción literal) y el checklist de ADR-0004 completado por personaje;
   - itera con `just sim --check` (release) ajustando **solo contenido** hasta que pasen los criterios de RF17 (incluido el 3 % por `MomentKind`, mientras game-designer no lo pase a aviso), y revisa la matriz y los avisos de 30–70 %;
   - si un criterio global no se alcanza con contenido, propone valores de `Ruleset` (paso 8) o ajustes de los umbrales permitidos por la decisión 16 (la fracción del 80 %, el tope de `time_up` —por encima del 10 %, con el OK del product-owner—, la banda del sesgo y el 3 % por `MomentKind`; nunca la banda de 45 000–90 000 ms), con el informe que lo justifica.

   El backend-engineer agrega `crates/content/tests/real_content.rs` y `crates/sim/tests/balance.rs` en el mismo commit que los personajes, para que cada commit pase `just check`. El checklist legal lo revisa alguien distinto del game-designer (ver "Pendiente de decidir").
8. **Ajuste del ruleset (solo si hace falta).** Protocolo de la decisión 14, en un commit propio y antes del commit final de contenido, para que el contenido se balancee contra los valores definitivos. Después se vuelve a correr `just sim --check`.
9. **Tiempo de los tests en debug.** Mido `cargo test -p sim --test balance` y `cargo test -p engine`. Si `balance` pasa de ~10 s en la Mac del owner, aplico `[profile.dev.package.engine] opt-level = 2` (ver Riesgos) y lo registro como desviación.
10. **Docs:** `docs/architecture.md`, desviaciones de este plan y, si cambiaron valores o umbrales, el pedido a architect de la enmienda editorial de la spec.
11. **Cierre:** `just check` completo con la salida pegada; `just sim` en release con su informe (incluido el tiempo del RNF) pegado; PR "feat(content,sim): characters, loader and balance simulator, phase B (TA-3)" con la línea propuesta de `CLAUDE.md`, el checklist de ADR-0004 por personaje y, si se regeneraron dorados, el aviso a 002.

## Riesgos
- **El balance no cierra con 2–4 personajes** (una debilidad ×5/4 inclina mucho un cruce; la aproximación en Python dio 28–75 % por personaje antes de ajustar contenido) → el game-designer ajusta stats, personalidad, habilidades y debilidades con `just sim`; RF17 es global y una matriz asimétrica solo avisa (preocupación 10). Si no alcanza, el paso 8.
- **El motor real diverge de la aproximación en Python** (duración, momentos, `time_up`) → `sim` mide el motor real; el paso 8 corrige valores sin cambiar fórmulas. Cambiar la **forma** de una fórmula enmienda la spec y queda fuera de este plan: si hace falta, paro y lo escalo a architect.
- **Ajustar el ruleset rompe los dorados o sus vectores** → protocolo de la decisión 14 en un commit aislado; los `covers` y `vectors_cover_every_event_and_moment` impiden perder una ruta (`time_up`, `draw`, cada momento) en silencio; no hay consumidores de los dorados todavía (Supuestos).
- **El test de balance alarga `just check`** (debug, ~0,5 ms por pelea según la desviación 1 de plan-a: con 4 personajes son 9 996 cruces más 3 332 espejos, unos 7 s, más `canonical_json`) → paso 9: si pasa de ~10 s, `opt-level = 2` para `engine` en dev. No cambia resultados (la aritmética es entera y `overflow-checks` sigue activo en dev, porque la sobreescritura por paquete solo cambia `opt-level`), y además acelera `properties.rs`. El costo es depurar el motor optimizado en los tests, aceptable.
- **El checklist de ADR-0004 lo firma quien diseña** (ADR-0004 dice que el game-designer revisa cada personaje, y aquí él es el autor) → lo revisa otro rol, y como es un tema legal (`risk: high`), con el OK del owner. Lo dejo como pregunta.
- **Nombres demasiado cercanos a una franquicia** (el ejemplo de la spec, `rata-electrica`, evoca a propósito) → checklist por personaje en el PR; el `slug` y el `archetype` describen el arquetipo, nunca el nombre original; sin sprites ni assets de terceros (nada en `content/_local/` entra al repo).
- **Deriva entre `abilities` del lore y los `id` del JSON** (no se parsea en 003) → el skill lo pone en el checklist del personaje y el revisor lo comprueba en el PR; la validación automática es de la feature que muestre los nombres.
- **Un consumidor activa features que el motor hereda en un build conjunto** → segundo árbol en `check-engine` (decisión 13). El build del server ya unifica `rand_core/os_rng` vía `tungstenite`, y eso lo sigue cubriendo el `clippy.toml` del motor (RF5).
- **El `clippy.toml` de I/O no dispara** (ruta mal escrita) → verificación manual del paso 2 con cada método, pegada en el PR.
- **Prettier rompe o reformatea fixtures** (un JSON roto a propósito) → `.prettierignore` para los fixtures de `content` y `sim`. `content/` ya está ignorado.
- **Informes no reproducibles** → la semilla raíz va en el informe, el orden de la decisión 7 es fijo y `Report` no contiene el tiempo; `tests/cli.rs` comprueba que dos corridas con la misma semilla dan el mismo `--json` (salvo `elapsed_ms`).
- **Postgres compartido con sesiones paralelas** → no se usa `db-reset`/`db-down`; los tests nuevos no tocan la base.

## Alternativas descartadas
- **Validar `archetype` y `rivals` copiando el patrón en `content`:** duplicaría una regla que RF12 pone en el motor. Se exporta `is_valid_slug` (decisión 2).
- **`load_roster` que devuelve solo `CharacterSpec`:** pierde `archetype` y `rivals`, que el server y el narrador necesitan, y obligaría a una segunda pasada por los archivos.
- **`clap`, `anyhow` o un crate de hex:** versiones nuevas en el lock, que el RNF prohíbe. El parseo a mano son unas decenas de líneas con tests.
- **Un constructor de `Ruleset` para experimentar desde `sim`** (`Ruleset::v1().with(...)`): rompe "solo se construye con `for_version`" y permite simular con valores que no son de ninguna versión. Se experimenta editando `v1()` en el árbol de trabajo.
- **Floats en el informe:** las comparaciones en el borde de un umbral dependerían del redondeo. Con conteos y multiplicación cruzada, el test es exacto (decisión 9).
- **Métricas sobre cruces y espejos juntos:** un espejo no es un cruce real; los espejos solo miden el sesgo de lado (decisión 8).
- **Un dorado sobre el contenido real para la ruta `time_up`:** acopla el balance a los dorados, que RF16 evita (decisión 15).
- **Chequeo textual de `std::fs` en `crates/engine/src`:** se esquiva con `use std::{fs as f}`; clippy resuelve rutas (decisión 13).
- **Paralelizar `sim` con hilos:** el RNF se cumple de sobra en un hilo (el motor tarda microsegundos por pelea en release) y un solo hilo mantiene el orden trivialmente determinista.
- **`sim` en `tools/`:** el filtro `rust` de CI cubre `crates/**` (lo dice la spec).

## Pruebas (cómo se demuestra que funciona)
- [ ] `just check` en verde, con la salida pegada en el PR.
- [ ] `just crate-age` y `just deny` en verde tras el paso 1; diff de `Cargo.lock` solo con `content` y `sim`.
- [ ] **RF12** (`crates/content/tests/load.rs`):
  - el roster válido carga 3 personajes ordenados por `slug` (los directorios se crean con nombres que no vienen ordenados en el FS), ignora el `README.md` y cada `spec` es exactamente el `CharacterSpec` esperado;
  - `canonical_spec_json` de un personaje cargado no contiene `archetype`, `ruleset_version` ni `rivals`, y coincide con el de la forma motor equivalente;
  - un caso por código de `ContentError`: campo desconocido arriba y en `stats` (`invalid_json`, con `line`/`column`), JSON roto, `slug_mismatch`, `archetype_invalid`, `ruleset_version_mismatch`, `missing_lore` para `es` y para `en`, `spec` con `budget_exceeded`, `missing_character_file`, `rival_unknown`, `rival_self` y `rival_duplicate`;
  - `unexpected_entry` (symlink) y `file_too_large` (65 KiB) se crean en `env!("CARGO_TARGET_TMPDIR")` durante el test, sin commitear archivos grandes ni enlaces;
  - ningún `Display` de `ContentError` contiene el texto de `serde_json` (por ejemplo, `expected` o `unknown field`), y `code()` es único y `snake_case`;
  - `is_valid_slug` (unit en `engine`) acepta y rechaza los mismos casos que `slug_pattern`.
- [ ] **RF13:** `crates/content/tests/real_content.rs` carga `content/characters/` con `Ruleset::for_version(1)`, exige de 2 a 4 personajes y falla con el `code()` y la ruta si alguno no es válido.
- [ ] **RF14:** los archivos de cada personaje existen (lo cubre RF13); el skill y el README están actualizados; el PR trae el checklist de ADR-0004 por personaje, revisado por alguien distinto del game-designer; en la revisión se comprueba que el `abilities` de cada lore tiene exactamente los `id` del JSON.
- [ ] **RF15** (unit en `crates/sim/src`):
  - `parse_seed_hex`: 64 caracteres en mayúsculas y minúsculas; 63, 65 o un carácter no hex dan error; la semilla por defecto en hex vuelve a sus 32 bytes;
  - `args`: valores por defecto, cada flag, un flag desconocido, `--fights` no numérico, 0 o por encima del tope;
  - planificación: con `n` = 2, 3 y 4 y `N` = 10 000, `P = C(n, 2)`, `m = ⌊N/(2·P)⌋`, `2·P·m ≤ N` cruces con las dos orientaciones de cada semilla, `n·m` espejos fuera de `N`; `RosterTooSmall` con 1 personaje y `TooFewFights` con `N < 2·P`;
  - derivación: la misma raíz da el mismo `Report`, y otra raíz da otro;
  - métricas sobre logs construidos a mano: medios puntos con empates, matriz y su aviso en 29/30/70/71 %, percentiles de rango más cercano, % por `MomentKind`, knockdown, densidad y la victoria tras ir ≥ 250 ‰ abajo (un caso que llega a 250 ‰ exacto cuenta, uno que llega a 249 ‰ no);
  - `failures()` en cada borde de cada criterio de `Criteria::RF17` (dentro y fuera), y `warnings()` nunca entra en `failures()`.
- [ ] **RF15** (`crates/sim/tests/cli.rs`): `--content <balanced> --fights 400 --check` sale con 0, `<lopsided>` con 1, un flag inválido o un contenido inválido con 2 (y el `code()` en stderr); `--json` es JSON válido con los campos del informe, la misma semilla da el mismo JSON (sin `elapsed_ms`) y el texto está en inglés.
- [ ] **RF17** (`crates/sim/tests/balance.rs`): 10 000 peleas con la semilla por defecto sobre `content/characters/`; `failures()` vacío (win rate de cada personaje en 400–600 ‰; ≥ 15 % con algún `moment`; ≥ 80 % con `end.t` en 45 000–90 000; `time_up` ≤ 10 %; `draw` < 1 %; lado `a` en 47–53 % de los espejos; y cada `MomentKind` ≥ 3 % de los cruces, salvo que game-designer lo pase a aviso). Si falla, el mensaje del `assert!` es el `render_text()` completo.
- [ ] **RNF de rendimiento:** `just sim` (release) en la Mac del owner reporta < 10 s para 10 000 peleas con `canonical_json`; el informe completo va en el PR (CI no lo asegura).
- [ ] **Pureza (decisión 13):** `check-engine.test.mjs` pasa con `tree-consumers-ok.txt` y falla con `rand_chacha feature "std"`, `rand_chacha feature "serde"` y `getrandom` en el árbol de los consumidores; `_check-ts-rs-graph` con `content` y `sim`; verificación manual del `clippy.toml` de I/O, pegada en el PR.
- [ ] **Dorados (RF10, RF16, RF18):** sin ajuste de ruleset, `cargo test -p engine` y `_check-wasm` pasan sin regenerar nada. Con ajuste, el commit de la decisión 14 regenera todo, los `covers` y `vectors_cover_every_event_and_moment` pasan (con `time_up` y `decision`), y `_check-wasm` pasa los mismos archivos en WASI.

## Estimación de CI
- `changes`/`policy`: sin cambios.
- `rust`: compilar `content` y `sim` (deps ya cacheadas, ~15 s en frío), sus tests (`content` < 1 s; `balance` ~7 s en debug, o ~2 s con `opt-level = 2`; `cli` ~3 s, que compila el bin en debug) y el segundo `cargo tree` de `check-engine` (~1 s). Unos **+30 a 45 s** por corrida, dentro del `timeout-minutes: 20`. Sin cambios de workflow.
- `web`: corre porque cambian `scripts/**` (y `crates/engine/**` si hay ajuste); +2 casos de Vitest, segundos.
- `images`: corre por `Cargo.toml` y `Cargo.lock`; la duración no cambia (el server no depende de los crates nuevos).
- `e2e`: no cambia.

## Pendiente de decidir (para la revisión)
- **architect (resuelto en la `## Revisión`):** (1) el criterio "cada `MomentKind` ≥ 3 %" como fallo, que extiende RF17 (decisión 10); (2) el endurecimiento de I/O del `clippy.toml` del motor (decisión 13); (3) la firma `load_roster(root, ruleset) -> Vec<Character>` y `is_valid_slug` (decisiones 1 y 2); (4) el universo "solo cruces" de las métricas (decisión 8); (5) que las enmiendas editoriales de la spec por valores o umbrales ajustados las haga architect al cerrar la fase.
- **game-designer:** aprobar la enmienda de RF17 (el 3 % por `MomentKind` como fallo y los umbrales ajustables), cuántos personajes (decisión 17) y si alguno de los umbrales ajustables de la decisión 16 necesita ajuste tras medir el motor real.
- **owner / product-owner:** quién revisa el checklist de ADR-0004, dado que el autor es el game-designer (propongo product-owner, más el OK del owner por ser tema legal).

## Desviaciones durante la implementación
Pasos 1 a 6 (backend-engineer, 2026-10-27). Ninguna cambia el formato del log, las fórmulas ni los dorados.
1. **`Character` con campos privados** (recomendación 3 de architect): getters `spec()`, `archetype()`, `rivals()` e `into_spec()`, así que un `Character` solo sale de `load_roster` y "validado" es un invariante del tipo.
2. **`invalid_shape` separado de `invalid_json`** (nota opcional 3 de architect), con `serde_json::Error::classify()`: un campo desconocido, uno que falta o un tipo incorrecto es `invalid_shape`; la sintaxis rota o el EOF es `invalid_json`. Los dos llevan solo `line` y `column`. Por eso los casos de campo desconocido (arriba y en `stats`) esperan `invalid_shape`, y se sumó el fixture `wrong-type`. `ContentError` suma también `path()`.
3. **Errores deterministas:** `load_roster` recorre las entradas de la raíz en orden de bytes del nombre antes de cargar, así que con dos personajes rotos el error es siempre el mismo en cualquier sistema de archivos (test `errors_are_reported_in_name_order`). Además de los casos del plan, `tests/load.rs` cubre el `character.json` exacto de 64 KiB (carga) y de 64 KiB + 1 (`file_too_large`), un directorio llamado `character.json`, `lore.*.md` y directorio de personaje como symlink, una raíz que no existe (`io` con `NotFound`) y, solo en Linux (APFS rechaza esos nombres), `name_not_utf8`.
4. **`checkEngineGraph(tree, { roots })`** en lugar de `{ requireEngineRoot: false }`: el segundo árbol exige que `content` y `sim` estén en la salida (si no, el `cargo tree` no revisó lo que creemos), y los mensajes nombran "the normal graph of content and sim" (nota 6 de architect). Por defecto `roots = ['engine']` y los mensajes de antes no cambian.
5. **`sim`:**
   - `SimError` suma `duplicate_slug` y `log_without_end` (guarda: el motor siempre escribe `end`), y los errores de uso son `ArgsError` con códigos `usage.*` (`unknown_flag`, `not_utf8`, `missing_value`, `repeated_flag`, `invalid_fights`, `invalid_seed`). `-h` es alias de `--help`.
   - `failures`, `warnings` y `render_text` reciben `&Criteria` (el bin y el test pasan `Criteria::RF17`), para probar el paso del 3 % de fallo a aviso (`moment_kind_fails`) sin tocar la constante.
   - La banda de 45 000–90 000 ms es `DURATION_BAND_MS`, fuera de `Criteria`, porque no es ajustable (decisión 16). `Criteria` guarda todo en ‰.
   - Los tests de las métricas viven en `crates/sim/src/report/tests.rs`.
   - Fixtures de `cli.rs`: `balanced` son dos copias del fixture `trickster` del motor con otro `slug` (nota 5 de architect: cada uno gana exactamente el 50 %); pasa `--check` con `--fights 400` y la semilla por defecto. `lopsided` es `trickster` contra un personaje con todos los stats en 1. `single` (un personaje) cubre `sim.roster_too_small`.
6. **Commits:** los pasos 1 y 4 van como `build:` y el 6 como `docs(content):`. Este registro va en un commit `docs(003)` aparte, después del paso 6.
7. **Dato para 002 (no es desviación):** con los dos `trickster` y 10 000 peleas, la separación media entre resoluciones es de 961 ms (p50 de 49 resoluciones por pelea). La densidad con el contenido real la mide el informe del paso 7.

## Revisión

### architect — 2026-10-27 — APPROVE
- Verifiqué:
  - **Frontmatter:** válido según `docs/process.md`. El autor (backend-engineer) no es el revisor, `reviewers: [architect]`, y `depends_on` y `linear` coinciden con plan-a.
  - **Cobertura de la fase B:** RF12 a RF15 y RF17, el RNF de rendimiento, las notas no bloqueantes de la `## Revisión` de la spec (el `kind` muerto, el aviso de 30–70 %, el sesgo en espejos, la victoria tras ir ≥ 250 ‰ abajo, `w_special_desperation_mul` y la ruta `time_up`) y la densidad para 002 (preocupación 8). Cada uno tiene archivo y prueba.
  - **Código actual (`236aea6`):**
    - `is_code` es privado de `spec.rs`.
    - `Ruleset::v1()` es `pub(crate)` y solo se construye con `for_version`. Los getters (`version`, `comeback_deficit_permille`…) existen.
    - `check_golden` es el único uso de `std::fs` y `std::env` en `crates/engine` (lo busqué con grep en `src/`, `tests/` y `tests/common/`).
    - `checkEngineGraph` hoy exige la raíz `engine`, así que hace falta la opción nueva.
    - `WORKSPACE_MEMBERS` y `_check-ts-rs-graph` son los que el plan extiende.
    - El filtro `rust` de `ci.yml` ya incluye `crates/**` y `content/**`.
  - **ADR-0001 y límites entre módulos:** la dirección `protocol ← engine ← {content, engine-wasm, sim}` se mantiene y el motor no gana dependencias. `content` solo hace I/O y conversión (la validación sigue en `validate`), y `sim` consume solo la API pública y el log. La victoria tras ir abajo se reconstruye del log y no de internos del motor. El tiempo vive en `main.rs` y `Report` es determinista.
  - **ADR-0002:** la semilla raíz que imprime `sim` es la del simulador, el CLI nunca recibe semillas reales y `Seed` sigue sin exponer bytes, porque `sim` guarda sus propios `[u8; 32]`.
  - **ADR-0004:** sin sprites, el checklist va por personaje y lo revisa alguien distinto del autor (ver abajo).
  - **ADR-0007:** el log no cambia, `ContentError` y `SimError` son códigos y nunca llevan el texto de `serde_json`, y el informe en inglés es una herramienta de desarrollo, como dice RF15.
  - **Aritmética:** los medios puntos y la multiplicación cruzada son correctos: 400–600 ‰ se escribe `800·apariciones ≤ 1000·(2·v + e) ≤ 1200·apariciones`. Con 4 personajes, `P = 6` y `m = 833`, así que hay 9 996 cruces y 3 332 espejos. El ±3 % del sesgo es unas 3,5 σ con 3 332 espejos.
  - **Protocolo de ajuste (decisión 14):** un commit propio, los `covers` y `vectors_cover_every_event_and_moment` impiden perder `time_up`, `draw` o un momento, `spec-*.json` no cambia y no hay consumidores de los dorados. Respeta RF10.
- **Respuestas a "Pendiente de decidir" (architect):**
  1. **"Cada `MomentKind` ≥ 3 %" falla en `--check` y en el test.** Un `kind` muerto es un defecto real para el narrador, y el umbral es bajo frente a lo que estimó la aproximación (9,1 / 10,8 / 20,4 %). Escribí la enmienda de RF17 en la spec. Es un cambio de criterio de gameplay, así que **queda pendiente de la aprobación de game-designer**. Si no la da, se implementa como aviso, que la decisión 10 ya prevé. El universo es "% de cruces con al menos un `moment` de ese `kind`".
  2. **Sí al endurecimiento de I/O del `clippy.toml` del motor**, con el `#[allow(clippy::disallowed_methods, reason = "…")]` local en `check_golden`. Tiene que ser `allow` y no `expect`, porque en wasm el bloque queda fuera por `cfg` y un `expect` sin cumplir avisaría. Sumen, en `disallowed-types`, `std::fs::File`, `std::fs::OpenOptions`, `std::process::Command`, `std::net::{TcpStream, TcpListener, UdpSocket}` y `std::thread::Builder`: un tipo prohibido cubre todos sus métodos, mejor que enumerarlos. En `disallowed-methods`, sumen las funciones libres que faltan: `std::fs::{metadata, create_dir_all, remove_file, copy, rename}` y `std::env::{vars_os, args_os, current_dir}`. Si el `allow` necesita también `disallowed_types`, va en la misma función. Lo registré en RF5 de la spec como enmienda editorial.
  3. **Firma aprobada:** `load_roster(root: &Path, ruleset: &Ruleset) -> Result<Vec<Character>, ContentError>`, con un `Character` que conserva `archetype` y `rivals`, y `pub fn is_valid_slug(&str) -> bool` exportado por el motor. Es la regla de formato que RF12 pone en el motor y no cambia los dorados. Condición: `validate` llama a `is_valid_slug` para el `slug`, así que hay una sola definición, y su doc dice que es formato y no juego. Lo registré en RF12. Recomendación no bloqueante: campos privados con getters (`spec()`, `archetype()`, `rivals()`, `into_spec()`) para que un `Character` solo salga de `load_roster` y "siempre validado" sea un invariante del tipo, que el server va a necesitar.
  4. **Aprobado: todas las métricas solo sobre cruces**, con los espejos solo para el sesgo del lado `a`. En producción no hay espejos, así que es lo representativo. Lo registré en RF15 como precisión editorial.
  5. **Sí, las enmiendas editoriales de la spec las hago yo al cerrar la fase.** Son la tabla del ruleset y el párrafo de la aproximación en Python (con las cifras reales de `sim`), RF21 (68 acciones, 1200 ms y 418) y la preocupación 8 si cambian los tiempos, y los umbrales de RF17 que ajuste el game-designer. El paso 10 me pasa los valores antes y después y el informe de `sim`. Son editoriales mientras no cambie la **forma** de una fórmula, y si cambia, se para y se escala (lo dice Riesgos).
- **Checklist de ADR-0004 (propuesta del orquestador):** es coherente con ADR-0004 y con el proceso, y no hace falta enmendar el ADR. ADR-0004 pide que cada personaje pase el checklist "¿se parece demasiado?", y el game-designer, como autor, lo sigue completando. La regla 1 de `docs/process.md` agrega que el autor no aprueba, así que lo aprueba otro agente: product-owner, cuyo rol ya incluye "nada de contenido con copyright". Al ser un tema legal (`risk: high`), además hace falta el OK del owner, que llega en el OK del merge. Para que quede registrado:
  - el skill `write-character` dice que el autor completa el checklist y que lo aprueba un agente distinto del autor (product-owner por defecto, o game-designer si el autor es otro);
  - el veredicto del product-owner es un comentario firmado en el PR, por personaje, que incluye la comprobación de que los `abilities` del lore coinciden con los `id` del JSON.

  Si el owner prefiere que el ADR lo diga de forma literal, alcanza con una línea en "Consecuencias", con su OK (gate de ADR), pero no bloquea.
- **Cambio que pido al aplicar esta revisión (sin re-revisión: es alinear el texto con la enmienda de la spec):** la decisión 16 y el paso 7 dicen que el game-designer puede ajustar los umbrales de "duración, `time_up` o sesgo". Desde la enmienda, solo puede ajustar la fracción del 80 %, el tope de `time_up` (por encima del 10 %, con el OK del product-owner), la banda del sesgo y el 3 %. La banda de 45 000–90 000 ms no se toca.
- No bloqueante (para la implementación o el PR):
  1. **Symlinks en todo el árbol (decisión 4):** usar `symlink_metadata` también para `character.json` y los `lore.*.md`. Un enlace en cualquiera de ellos da `unexpected_entry`, no solo en la raíz, y el tope de 64 KiB se aplica leyendo con `take(MAX_FILE_BYTES + 1)`, no confiando en `metadata().len()`. Sumen un caso de test con el `character.json` como symlink.
  2. **`ContentError::Io`:** guardar el `io::ErrorKind` y la ruta, no el `io::Error`, cuyo `Display` trae texto del sistema operativo. `code()` sigue siendo `io`.
  3. **Opcional:** separar `invalid_json` (sintaxis o EOF) de `invalid_shape` (campo desconocido o tipo incorrecto) con `serde_json::Error::classify()`, sin usar el mensaje. Facilita corregir el archivo.
  4. **`sim` sin `HashMap`:** el `RandomState` cambia el orden en cada proceso. `Report` va con `Vec` indexado por el orden del roster o con `BTreeMap`. El test de dos corridas de `cli.rs` lo detectaría, pero de forma intermitente.
  5. **Fixture `balanced` de `cli.rs`:** dos personajes con el mismo spec y distinto `slug`. Como cada semilla se juega en las dos orientaciones, cada uno gana exactamente 500 ‰ sea cual sea el ruleset (el `slug` no entra en la simulación), así que el test no se rompe al ajustar valores en el paso 8. El sesgo de espejos y el 3 % siguen dependiendo de la muestra: si un ajuste los rompe, se cambia la `--seed` del test y se registra como desviación.
  6. **Mensajes de `checkEngineGraph` con `requireEngineRoot: false`:** que nombren la raíz revisada (`content`/`sim`) en lugar de "the normal graph of engine". Esto prohíbe para siempre `tokio`, `tracing` y `log` en `content`. Si el server lo necesitara, usa `spawn_blocking` y registra él (ADR-0012), y `content` sigue sin logging.
  7. **Rutas de los tests sobre el contenido real:** `real_content.rs` y `balance.rs` resuelven `content/characters` con `concat!(env!("CARGO_MANIFEST_DIR"), "/../../content/characters")`, no relativo al cwd.
  8. **`LOCALES` de `content`:** un comentario que apunte a la lista de locales de la web (ADR-0007), para que un tercer idioma no se olvide aquí.
  9. **Paso 7:** el orquestador es quien invoca al game-designer, no el backend-engineer. Que el plan lo diga así.
- Gate humano: el plan es `risk: high`, así que según `docs/process.md` falta el OK explícito del owner en chat antes de empezar la implementación. Esta aprobación no lo reemplaza. La enmienda de RF17 (el 3 % y los umbrales ajustables) espera además la aprobación de game-designer, y hasta entonces el criterio del 3 % se implementa detrás de `Criteria::RF17`, de modo que pasar de fallo a aviso sea un cambio de una línea.
