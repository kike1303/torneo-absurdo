import { parseFightId } from '#lib/features/replay/fight-id.js';

// Before any read: what does not pass is a 404 without touching the API (011 RF18).
export function match(param: string): boolean {
  return parseFightId(param) !== null;
}
