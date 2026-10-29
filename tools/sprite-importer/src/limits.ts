// The few limits of the tool itself (007 plan-b decision 11). Everything else is read from
// GUIDE_LIMITS and LIMITS, never copied.
import { GUIDE_LIMITS } from '@torneo/renderer/guide';
import { LIMITS } from '@torneo/renderer/manifest';

const cellsPerSide = Math.floor(LIMITS.sheetSideMax / GUIDE_LIMITS.cellPx);

export const TOOL_LIMITS = Object.freeze({
  /** manifest.json, provenance.json and character.json (RF-B3; `content::MAX_FILE_BYTES`). */
  jsonFileMaxBytes: 64 * 1024,
  /** Cells per row of a sheet (10 with 192). */
  columns: cellsPerSide,
  /** Rows of cells per sheet (10 with 192). */
  rows: cellsPerSide,
});

/** The pivot the standard fixes (RF-B1): the export never declares it. */
export const PIVOT = Object.freeze({
  x: GUIDE_LIMITS.cellPx / 2,
  y: GUIDE_LIMITS.cellPx - GUIDE_LIMITS.groundMarginPx,
});
