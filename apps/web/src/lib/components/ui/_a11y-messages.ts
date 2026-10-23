// The only file in `ui/` that may import Paraglide messages (ADR-0009 D4 and D8): the texts that
// shadcn ships in English ("Close", `sr-only` labels) come from here.
import { m } from '#lib/paraglide/messages.js';

export const a11y = {
  close: (): string => m.a11y_close(),
  more: (): string => m.a11y_more(),
};
