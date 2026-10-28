# Personajes

Cada personaje vive en `content/characters/<slug>/`, con el nombre del directorio igual a su `slug`:

```
<slug>/
  character.json   # datos de juego (stats, personalidad, habilidades, debilidades, rivales): solo códigos y enteros
  lore.es.md       # nombre, epíteto, nombres de habilidades (frontmatter `abilities`) y lore en español
  lore.en.md       # lo mismo en inglés, con el humor adaptado (no es una traducción literal)
  sprites/         # opcional, desde la feature 004
```

- El formato exacto, el checklist legal "¿se parece demasiado?" (ADR-0004) y quién lo aprueba están en el skill [`write-character`](../../.claude/skills/write-character/SKILL.md). El esquema lo fija la spec del motor ([003](../../docs/features/003-motor-de-pelea/spec.md), "Esquema de personaje").
- `crates/content` (`load_roster`) carga este directorio: ignora los archivos sueltos como este README, rechaza symlinks y deja todas las reglas de juego al motor. Un test (`crates/content/tests/real_content.rs`) lo carga dentro de `just check`, así que un personaje inválido no pasa CI.
- Balance: `just sim --check` corre 10 000 peleas con el motor real y falla si no se cumplen los criterios de RF17 (win rate de 40–60 % por personaje, duración, `time_up`, momentos). El mismo chequeo corre en `just check` (`crates/sim/tests/balance.rs`).

Los sprites de MUGEN o de franquicias **nunca** van aquí; solo en `content/_local/` (ignorado por git). Ver [ADR-0004](../../docs/decisions/0004-personajes-homenaje.md).
