# Catálogo de la botarga del jugador

Lo que una botarga de jugador puede equipar ([feature 016](../../docs/features/016-botarga-del-jugador/feature.md)). Solo códigos y enteros en los JSON de juego; los nombres van aparte, por idioma.

```
abilities.json     # ataques: kind, power, cooldown, element, tier (1-5) y un único starter
accessories.json   # accesorios: slot (head, hands, torso, legs, feet), tier y deltas de stats de suma cero
styles.json        # estilos de pelea: presets de personalidad
names.es.json      # nombres en español de cada ataque, accesorio y estilo
names.en.json      # lo mismo en inglés, con el humor adaptado
```

- `crates/content` (`load_catalog`) lo carga y lo valida contra el ruleset v2 dentro de `just check` (`crates/content/tests/catalog.rs`). Las reglas de un ataque y de un accesorio son del motor; el cargador agrega ids únicos, tiers en rango, un solo starter (pesado, tier 1, sin elemento) y un nombre por entrada en cada idioma.
- El tier no da fuerza: poder y cooldown se pagan entre sí. Un accesorio mueve puntos entre stats (como mucho ±10 por stat) y nunca suma.
- Tras editarlo, mide el balance con el simulador (fase 3 de la feature 016).
