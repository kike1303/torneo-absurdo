# Criterios de code review

Lo usan los agentes revisores (ver skill `review-pr`). Cada hallazgo se etiqueta con su pase.

## Passes
1. **Bugs**: errores de lógica, condiciones de carrera, manejo de errores, casos límite.
2. **Seguridad**: auth y sesiones, validación de entrada, secretos, filtración de la semilla o de eventos futuros, manipulación de monedas o apuestas, rate limiting.
3. **Determinismo**: el motor sigue siendo puro y reproducible (misma semilla → mismo log), sin reloj, sin `HashMap` con orden no determinista, sin floats dependientes de la plataforma en la lógica del resultado.
4. **Cumplimiento**: el diff coincide con `plan.md` y `spec.md`; cualquier desvío está documentado en el plan.
5. **i18n y UI**: sin strings literales, los componentes usan tokens del theme, mobile-first, accesibilidad básica.

## Qué es "Important" aquí
Algo que rompe comportamiento, filtra datos o resultados, permite crear o duplicar monedas, rompe el determinismo o viola un ADR aceptado. El estilo y los nombres son **nits**.

## Regla de RF6 (spec de 010)
Cambiar o quitar una entrada de la tabla de `apps/server/tests/ruleset_freeze.rs` es **bloqueante**: rompería la verificación de las peleas pasadas de esa versión. Un cambio de reglas lleva una versión nueva.

## Límite de nits
Máximo 5 por review. El resto se resume como un conteo.

## No reportar
- Archivos generados (`packages/protocol-ts/**`, lockfiles).
- Lo que ya hacen CI, rustfmt, clippy o prettier.

## Firma
Cada comentario empieza con `**🤖 <agente>**` y termina con `Veredicto: APPROVE | REQUEST_CHANGES | COMMENT`.
