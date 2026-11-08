import { defineParams } from '@sveltejs/kit/params';
import { parseFightId } from './lib/features/replay/fight-id.js';

// Route matchers, in the one file of Kit 3. `fight_id` runs before any read: what does not pass is a
// 404 without touching the API (011 RF18). The value stays the text, and the `load` parses it.
export const params = defineParams({
  fight_id: (param: string) => (parseFightId(param) === null ? undefined : param),
});
