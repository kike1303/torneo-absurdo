import type { Provider } from '@torneo/protocol';

/** The order of the providers on screen. */
const ORDER = ['google', 'discord', 'dev'] as const satisfies readonly Provider[];

export function isProvider(value: unknown): value is Provider {
  return typeof value === 'string' && (ORDER as readonly string[]).includes(value);
}

export interface ProviderRow {
  provider: Provider;
  linked: boolean;
  /**
   * Whether unlinking leaves at least one other linked provider that is enabled (RF-C3). The
   * server decides anyway (409 `auth.last_provider`); this only disables the button.
   */
  canUnlink: boolean;
}

/**
 * The rows of the account (plan-d decision 9): every enabled provider and every linked one, in a
 * fixed order. `enabled` is `null` when `GET /api/auth/providers` did not answer: then only the
 * linked providers are listed, and every one of them counts as a way to sign in.
 */
export function providerRows(
  linked: readonly Provider[],
  enabled: readonly Provider[] | null,
): ProviderRow[] {
  const isEnabled = (p: Provider) => enabled === null || enabled.includes(p);
  const listed = (p: Provider) => linked.includes(p) || (enabled !== null && enabled.includes(p));
  return ORDER.filter(listed).map((provider) => {
    const isLinked = linked.includes(provider);
    const others = linked.filter((p) => p !== provider && isEnabled(p));
    return { provider, linked: isLinked, canUnlink: isLinked && others.length > 0 };
  });
}
