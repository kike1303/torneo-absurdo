// Shelf packing (002 RF-B5, plan-b decision 10). Pure and deterministic: tallest first, then
// widest, then by key; 1 px of transparent margin; a new sheet when the height runs out.
import { fail } from './errors.ts';
import { INPUT_LIMITS } from './limits.ts';

export interface PackItem {
  readonly key: string;
  readonly width: number;
  readonly height: number;
}

export interface Placement {
  readonly sheet: number;
  readonly x: number;
  readonly y: number;
}

export interface PackResult {
  readonly placements: ReadonlyMap<string, Placement>;
  readonly sheets: readonly { readonly width: number; readonly height: number }[];
}

export const MARGIN = 1;

export function pack(
  items: readonly PackItem[],
  side: number = INPUT_LIMITS.sheetSide,
): PackResult {
  const sorted = [...items].sort(
    (a, b) =>
      b.height - a.height || b.width - a.width || (a.key < b.key ? -1 : a.key > b.key ? 1 : 0),
  );
  const placements = new Map<string, Placement>();
  const sheets: { width: number; height: number }[] = [];
  let sheet = -1;
  let x = 0;
  let y = 0;
  let shelf = 0;
  const open = () => {
    sheet++;
    if (sheet >= INPUT_LIMITS.sheets) fail('convert.too_many_sheets', { sheets: sheet + 1 });
    sheets.push({ width: 0, height: 0 });
    x = 0;
    y = 0;
    shelf = 0;
  };
  for (const item of sorted) {
    if (item.width > side || item.height > side) {
      fail('convert.frame_too_large', { width: item.width, height: item.height });
    }
    if (sheet === -1) open();
    if (x > 0 && x + item.width > side) {
      y += shelf + MARGIN;
      x = 0;
      shelf = 0;
    }
    if (y + item.height > side) open();
    placements.set(item.key, { sheet, x, y });
    const current = sheets[sheet];
    if (current !== undefined) {
      current.width = Math.max(current.width, x + item.width);
      current.height = Math.max(current.height, y + item.height);
    }
    x += item.width + MARGIN;
    shelf = Math.max(shelf, item.height);
  }
  return { placements, sheets };
}
