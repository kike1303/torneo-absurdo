// The generated block of docs/guides/sprites.md (007 RF-B7, plan-b decision 24): three markdown
// tables from GUIDE_LIMITS, guideTable(rulesets), LIMITS, TIMELINE_LIMITS, REACTION_WINDOW_MS and
// CONTACT_PX. The headers are Spanish constants: this is a documentation generator, not UI nor
// domain (ADR-0007 applies to the UI, the engine and the protocol). Pure.
import {
  CONTACT_PX,
  GUIDE_LIMITS,
  TIMELINE_LIMITS,
  guideTable,
  type GuideRow,
  type RulesetTiming,
} from '@torneo/renderer/guide';
import {
  ANIMATION_FALLBACK,
  LIMITS,
  REACTION_WINDOW_MS,
  type OptionalAnimation,
} from '@torneo/renderer/manifest';
import { TOOL_LIMITS } from './limits.ts';

export const GUIDE_START = '<!-- guide:generated:start -->';
export const GUIDE_END = '<!-- guide:generated:end -->';

/** Recommendations, not validated (spec, "¿Paleta y contorno se validan?" and point 3). */
export const RECOMMENDED = Object.freeze({
  paletteColorsMax: 32,
  outlinePx: 1,
  idleCycleMinMs: 800,
  idleCycleMaxMs: 1600,
});

const MIB = 1024 * 1024;
const DASH = '—';

function table(header: readonly string[], rows: readonly (readonly string[])[]): string {
  const line = (cells: readonly string[]) => `| ${cells.join(' | ')} |`;
  return [line(header), line(header.map(() => '---')), ...rows.map(line)].join('\n');
}

const range = (min: number, max: number, unit = '') => `${min}–${max}${unit}`;
const code = (text: string) => `\`${text}\``;

function cellTable(): string {
  const g = GUIDE_LIMITS;
  const pivotX = g.cellPx / 2;
  const pivotY = g.cellPx - g.groundMarginPx;
  return table(
    ['Parámetro', 'Valor', 'Fuente'],
    [
      ['Celda', `${g.cellPx} × ${g.cellPx} px`, code('GUIDE_LIMITS.cellPx')],
      [
        'Pivote (pies)',
        `(${pivotX}, ${pivotY})`,
        `${code('cellPx / 2')}, ${code('cellPx − groundMarginPx')}`,
      ],
      ['Margen bajo los pies', `${g.groundMarginPx} px`, code('GUIDE_LIMITS.groundMarginPx')],
      [
        'Alto opaco de `idle` (frame 0)',
        range(g.heightMinPx, g.heightMaxPx, ' px'),
        code('GUIDE_LIMITS.heightMinPx–heightMaxPx'),
      ],
      [
        'Fila opaca más baja de cada frame de `idle`',
        range(pivotY - 1 - g.feetTolerancePx, pivotY - 1),
        code('pivotY − 1 − feetTolerancePx … pivotY − 1'),
      ],
      [
        'Alcance en el frame de impacto',
        `≥ ${CONTACT_PX / 2} px delante del pivote (columna ≥ ${pivotX + CONTACT_PX / 2})`,
        code('CONTACT_PX / 2'),
      ],
      [
        'Duración de cada frame',
        range(g.frameMsMin, g.frameMsMax, ' ms'),
        code('GUIDE_LIMITS.frameMsMin–frameMsMax'),
      ],
      ['Pose clave mínima', `${g.keyPoseMinMs} ms`, code('GUIDE_LIMITS.keyPoseMinMs')],
      ['Ventana de reacción', `${REACTION_WINDOW_MS} ms`, code('REACTION_WINDOW_MS')],
      [
        'Separación mínima entre impactos',
        `${TIMELINE_LIMITS.strikeGapMs} ms`,
        code('TIMELINE_LIMITS.strikeGapMs'),
      ],
      [
        'Tope estructural de windup',
        `${LIMITS.windupMaxMs} ms`,
        code('TIMELINE_LIMITS.strikeGapMs − REACTION_WINDOW_MS'),
      ],
      [
        'Paso mínimo de windup de `attack_light` a `attack_heavy`',
        `${g.windupStepMs} ms`,
        code('GUIDE_LIMITS.windupStepMs'),
      ],
      [
        'Frames por animación (formato)',
        `≤ ${LIMITS.animationFramesMax}`,
        code('LIMITS.animationFramesMax'),
      ],
      [
        'Hojas por set',
        `≤ ${LIMITS.sheetsMax}, de ${TOOL_LIMITS.columns} × ${TOOL_LIMITS.rows} celdas`,
        `${code('LIMITS.sheetsMax')}, ${code('LIMITS.sheetSideMax')}`,
      ],
      [
        'Peso de cada hoja',
        `${g.sheetFileMaxBytes / MIB} MiB`,
        code('GUIDE_LIMITS.sheetFileMaxBytes'),
      ],
      [
        'Peso de cada archivo del export',
        `${g.exportFileMaxBytes / MIB} MiB`,
        code('GUIDE_LIMITS.exportFileMaxBytes'),
      ],
      [
        'Peso de `manifest.json` y `provenance.json`',
        `${TOOL_LIMITS.jsonFileMaxBytes / 1024} KiB`,
        code('TOOL_LIMITS.jsonFileMaxBytes'),
      ],
    ],
  );
}

function required(row: GuideRow): string {
  if (row.required) return 'sí';
  return `no (usa ${code(ANIMATION_FALLBACK[row.animation as OptionalAnimation])})`;
}

function frames(row: GuideRow): string {
  const base = range(row.frames.min, row.frames.max);
  return row.distinct_min === undefined ? base : `${base} (≥ ${row.distinct_min} distintos)`;
}

function duration(row: GuideRow): string {
  const parts: string[] = [];
  if (row.cycle_ms !== undefined)
    parts.push(`ciclo ${range(row.cycle_ms.min, row.cycle_ms.max, ' ms')}`);
  if (row.frame_max_ms !== undefined) parts.push(`cada frame ≤ ${row.frame_max_ms} ms`);
  const d = row.duration_ms;
  if (d !== undefined) parts.push(d.min === 0 ? `≤ ${d.max} ms` : range(d.min, d.max, ' ms'));
  return parts.length === 0 ? DASH : parts.join('; ');
}

function windup(row: GuideRow): string {
  const w = row.windup_ms;
  if (w === undefined) return DASH;
  const base = range(w.min, w.max, ' ms');
  return w.ruleset_version === undefined
    ? base
    : `${base} (tope del ruleset v${w.ruleset_version})`;
}

function impact(row: GuideRow): string {
  if (row.impact_frame_min === undefined) return DASH;
  return `${code('impact_frame')} ≥ ${row.impact_frame_min}; dura ≥ ${row.impact_hold_min_ms ?? 0} ms`;
}

function recovery(row: GuideRow): string {
  if (row.recovery_min_ms === undefined) return DASH;
  const recommended = row.recovery_recommended_max_ms;
  return recommended === undefined
    ? `≥ ${row.recovery_min_ms} ms`
    : `≥ ${row.recovery_min_ms} ms (recomendada ≤ ${recommended} ms)`;
}

function actionTable(rulesets: readonly RulesetTiming[]): string {
  return table(
    [
      'Acción',
      'Obligatoria',
      'Frames',
      'Duración',
      'Windup',
      'Impacto',
      'Recuperación',
      'Primer frame',
    ],
    guideTable(rulesets).map((row) => [
      code(row.animation),
      required(row),
      frames(row),
      duration(row),
      windup(row),
      impact(row),
      recovery(row),
      row.first_frame_max_ms === undefined ? DASH : `≤ ${row.first_frame_max_ms} ms`,
    ]),
  );
}

function recommendationTable(): string {
  const r = RECOMMENDED;
  return table(
    ['Recomendación (no validada)', 'Valor', 'Cómo se mide'],
    [
      [
        'Colores por set (pixel art)',
        `≤ ${r.paletteColorsMax}`,
        `${code('colors')} del resumen del importador`,
      ],
      ['Alfa', 'binaria (0 o 255)', `${code('partial_alpha_pixels')} = 0 en el resumen`],
      ['Contorno', `oscuro, de ${r.outlinePx} px lógico`, DASH],
      [
        'Ciclo de `idle`',
        range(r.idleCycleMinMs, r.idleCycleMaxMs, ' ms'),
        `suma de ${code('frame_ms')}`,
      ],
    ],
  );
}

/** The text between the markers: a blank line, the three tables, a blank line. */
export function renderGuideBlock(rulesets: readonly RulesetTiming[]): string {
  return [
    '',
    '### Celda y encuadre',
    '',
    cellTable(),
    '',
    '### Por acción',
    '',
    actionTable(rulesets),
    '',
    '### Recomendaciones',
    '',
    recommendationTable(),
    '',
  ].join('\n');
}

/** Replaces the block between the markers; `null` when they are missing or repeated. */
export function replaceGuideBlock(text: string, block: string): string | null {
  const start = text.indexOf(GUIDE_START);
  const end = text.indexOf(GUIDE_END);
  if (start < 0 || end < start) return null;
  if (text.indexOf(GUIDE_START, start + 1) >= 0 || text.indexOf(GUIDE_END, end + 1) >= 0)
    return null;
  return text.slice(0, start + GUIDE_START.length) + '\n' + block + '\n' + text.slice(end);
}

/** The block between the markers, or `null` (same rules as replaceGuideBlock). */
export function extractGuideBlock(text: string): string | null {
  const start = text.indexOf(GUIDE_START);
  const end = text.indexOf(GUIDE_END);
  if (start < 0 || end < start) return null;
  if (text.indexOf(GUIDE_START, start + 1) >= 0 || text.indexOf(GUIDE_END, end + 1) >= 0)
    return null;
  const inner = text.slice(start + GUIDE_START.length, end);
  return inner.startsWith('\n') && inner.endsWith('\n') ? inner.slice(1, -1) : null;
}
