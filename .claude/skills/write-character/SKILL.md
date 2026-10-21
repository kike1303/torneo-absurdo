---
name: write-character
description: Create or edit a Torneo Absurdo homage character (stats, abilities, personality, bilingual lore) in content/characters/<slug>/ in the standard format, including the "too similar?" legal checklist. Use whenever designing or changing a fighter.
---

# Escribir un personaje

## Estructura
```
content/characters/<slug>/
  character.yaml     # datos de juego (sin texto humano)
  lore.es.md         # nombre, epíteto y lore en español
  lore.en.md         # nombre, epíteto y lore en inglés
  sprites/           # manifest.json + spritesheets (formato del renderer)
```

## character.yaml
```yaml
slug: rata-electrica          # estable, en kebab-case; nunca cambia
archetype: electric-rodent     # el arquetipo que evoca
ruleset_version: 1
stats:                         # 1–100; la suma debe respetar el presupuesto del ruleset
  hp: 60
  attack: 55
  defense: 40
  speed: 85
  special: 70
personality:                   # pesos de comportamiento, 0–1
  aggression: 0.7
  caution: 0.2
  showmanship: 0.9
abilities:
  - id: thunder_squeak         # códigos, no texto
    kind: special
    power: 70
    cooldown: 3
    tags: [electric, stun]
weaknesses: [ground]
rivals: []                     # slugs, para el narrador
```
El esquema exacto lo fija la spec del motor (feature 003); si cambia, actualiza este skill.

## lore.<locale>.md
```markdown
---
name: Chispa
epithet: La Rata que Muerde Rayos
locale: es
---
<Lore de 80–150 palabras: origen, motivación, un rasgo ridículo y un rival.>

## Frases
- intro: "..."
- victoria: "..."
```
Se escribe **por idioma**, adaptando el humor; no es una traducción literal.

## Checklist "¿se parece demasiado?" (ADR-0004), obligatorio
- [ ] El nombre no es el original ni una variación obvia (nada de "Pikachú" ni "Gokú").
- [ ] Sin frases icónicas literales ni ataques con su nombre original.
- [ ] Diseño visual distinto: silueta, paleta y vestimenta no son calcos.
- [ ] Sin logos, símbolos ni marcas registradas.
- [ ] Se reconoce el **arquetipo**, no al personaje concreto.

## Balance
Después de crear o editar, el game-designer corre el simulador masivo del motor (cuando exista) y confirma un win rate global de 40–60%.
