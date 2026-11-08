import type { BetQuote, Side } from '@torneo/protocol';

// 012 RF47, plan-c C4: the estimate before confirming. One request when the amount or the side
// changes, after 300 ms without changes, and never because of a `pool_update`: that only marks the
// estimate as stale, with a way to compute it again. An answer for another amount or side is never
// shown. With an error, a 429 or no JS, there is no estimate, and the bet still goes.

export const QUOTE_DEBOUNCE_MS = 300;
/**
 * The server answers from its cache of pools, refreshed every 2 s (012 RF35): right after a window
 * opens it does not know the fight yet and says `bet.window_closed`. One retry after a refresh.
 */
export const QUOTE_RETRY_MS = 2500;

export interface QuoteInput {
  readonly fightId: number;
  readonly side: Side;
  readonly amount: number;
}

export type QuoteState =
  | { readonly status: 'idle' }
  | { readonly status: 'loading'; readonly input: QuoteInput }
  | { readonly status: 'ready'; readonly input: QuoteInput; readonly quote: BetQuote }
  | { readonly status: 'stale'; readonly input: QuoteInput; readonly quote: BetQuote }
  /** A 422 `bet.below_minimum` while typing: the minimum as a hint, never a refusal. */
  | { readonly status: 'below_minimum'; readonly input: QuoteInput }
  | { readonly status: 'unavailable'; readonly input: QuoteInput };

export interface QuoteDeps {
  readonly fetchFn: typeof fetch;
  readonly setTimer: (run: () => void, ms: number) => unknown;
  readonly clearTimer: (timer: unknown) => void;
  readonly onChange: (state: QuoteState) => void;
}

const same = (a: QuoteInput, b: QuoteInput): boolean =>
  a.fightId === b.fightId && a.side === b.side && a.amount === b.amount;

function isQuote(value: unknown, input: QuoteInput): value is BetQuote {
  if (typeof value !== 'object' || value === null) return false;
  const quote = value as Record<string, unknown>;
  return (
    quote.fight_id === input.fightId &&
    quote.side === input.side &&
    quote.amount === input.amount &&
    Number.isSafeInteger(quote.payout) &&
    Number.isSafeInteger(quote.net) &&
    typeof quote.one_sided === 'boolean' &&
    Number.isSafeInteger(quote.rake_bp)
  );
}

async function errorCode(response: Response): Promise<unknown> {
  try {
    const body: unknown = await response.json();
    return typeof body === 'object' && body !== null
      ? (body as Record<string, unknown>).error
      : undefined;
  } catch {
    return undefined;
  }
}

export interface QuoteClient {
  /** The amount or the side changed: a request after the wait, or nothing for an empty amount. */
  request(input: QuoteInput | null): void;
  /** A `pool_update` of this fight arrived: the estimate is stale, and nothing is asked. */
  poolChanged(fightId: number): void;
  /** Compute the stale estimate again, now. */
  refresh(): void;
  stop(): void;
}

export function createQuoteClient(deps: QuoteDeps): QuoteClient {
  let state: QuoteState = { status: 'idle' };
  let wanted: QuoteInput | null = null;
  let timer: unknown = null;
  /** A `pool_update` arrived while asking: the answer is already stale when it comes. */
  let movedWhileLoading = false;

  const set = (next: QuoteState): void => {
    state = next;
    deps.onChange(next);
  };

  const cancel = (): void => {
    if (timer !== null) deps.clearTimer(timer);
    timer = null;
  };

  const fetchQuote = async (input: QuoteInput, retried: boolean): Promise<void> => {
    const path = `/api/fights/${input.fightId}/quote?side=${input.side}&amount=${input.amount}`;
    let next: QuoteState;
    try {
      const response = await deps.fetchFn(path, {
        headers: { accept: 'application/json' },
        cache: 'no-store',
      });
      if (response.ok) {
        const body: unknown = await response.json();
        next = isQuote(body, input)
          ? { status: 'ready', input, quote: body }
          : { status: 'unavailable', input };
      } else {
        const code = await errorCode(response);
        if (response.status === 422 && code === 'bet.below_minimum') {
          next = { status: 'below_minimum', input };
        } else if (response.status === 409 && code === 'bet.window_closed' && !retried) {
          if (wanted !== null && same(wanted, input)) {
            timer = deps.setTimer(() => {
              timer = null;
              void fetchQuote(input, true);
            }, QUOTE_RETRY_MS);
          }
          return;
        } else {
          next = { status: 'unavailable', input };
        }
      }
    } catch {
      next = { status: 'unavailable', input };
    }
    // Out of order: only the answer for what is on screen now.
    if (wanted === null || !same(wanted, input)) return;
    if (next.status === 'ready' && movedWhileLoading) {
      next = { status: 'stale', input, quote: next.quote };
    }
    movedWhileLoading = false;
    set(next);
  };

  const start = (input: QuoteInput): void => {
    movedWhileLoading = false;
    set({ status: 'loading', input });
    void fetchQuote(input, false);
  };

  return {
    request(input) {
      cancel();
      wanted = input;
      if (input === null || input.amount <= 0) {
        wanted = null;
        set({ status: 'idle' });
        return;
      }
      timer = deps.setTimer(() => {
        timer = null;
        start(input);
      }, QUOTE_DEBOUNCE_MS);
    },
    poolChanged(fightId) {
      if (state.status === 'ready' && state.input.fightId === fightId) {
        set({ status: 'stale', input: state.input, quote: state.quote });
      } else if (state.status === 'loading' && state.input.fightId === fightId) {
        movedWhileLoading = true;
      }
    },
    refresh() {
      if (state.status === 'stale' && wanted !== null && same(state.input, wanted)) {
        cancel();
        start(wanted);
      }
    },
    stop() {
      cancel();
      wanted = null;
    },
  };
}
