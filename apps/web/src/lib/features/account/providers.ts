import type { Provider, ReauthStatus } from '@torneo/protocol';

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
  /**
   * What the confirmation panel of this row offers (009 RF21): its `reauth_with`, or, without
   * `reauth-status`, the linked providers that are enabled. Empty on a row that is not linked.
   */
  reauthWith: Provider[];
  /** Linked, and `reauth-status` says nothing can confirm its removal (009 RF21, Area 5). */
  blocked: boolean;
}

/**
 * The rows of the account (plan-d decision 9): every enabled provider and every linked one, in a
 * fixed order. `enabled` is `null` when `GET /api/auth/providers` did not answer: then only the
 * linked providers are listed, and every one of them counts as a way to sign in. `reauth` is
 * `null` when `GET /api/auth/reauth-status` did not answer: then nothing is blocked and the
 * action decides (009 RF21). The rule itself stays in the server.
 */
export function providerRows(
  linked: readonly Provider[],
  enabled: readonly Provider[] | null,
  reauth: ReauthStatus | null = null,
): ProviderRow[] {
  const isEnabled = (p: Provider) => enabled === null || enabled.includes(p);
  const listed = (p: Provider) => linked.includes(p) || (enabled !== null && enabled.includes(p));
  const linkedEnabled = ORDER.filter((p) => linked.includes(p) && isEnabled(p));
  return ORDER.filter(listed).map((provider) => {
    const isLinked = linked.includes(provider);
    const others = linked.filter((p) => p !== provider && isEnabled(p));
    const status = reauth?.unlink.find((u) => u.provider === provider);
    return {
      provider,
      linked: isLinked,
      canUnlink: isLinked && others.length > 0,
      reauthWith: isLinked ? (status?.reauth_with ?? linkedEnabled) : [],
      blocked: isLinked && status?.reauth_with.length === 0,
    };
  });
}

/**
 * The providers of the panel that replaces the link buttons (009 RF20): the linked and enabled
 * ones, only if the session has no confirmation and there is something left to link. Empty
 * otherwise, so without `reauth-status` the buttons stay and the server decides.
 */
export function linkPanel(
  reauth: ReauthStatus | null,
  linked: readonly Provider[],
  enabled: readonly Provider[] | null,
): Provider[] {
  if (reauth === null || reauth.link_ready || enabled === null) return [];
  if (!enabled.some((p) => !linked.includes(p))) return [];
  return ORDER.filter((p) => linked.includes(p) && enabled.includes(p));
}

/**
 * Whether a link would end in `auth.reauth_required` with nothing that can confirm it (TA-31, 009
 * RF20): no confirmation, something left to link, and no linked provider is enabled. Then neither
 * the buttons nor the panel can help. False without `reauth-status`: the action decides.
 */
export function linkBlocked(
  reauth: ReauthStatus | null,
  linked: readonly Provider[],
  enabled: readonly Provider[] | null,
): boolean {
  if (reauth === null || reauth.link_ready || enabled === null) return false;
  return (
    enabled.some((p) => !linked.includes(p)) && linkPanel(reauth, linked, enabled).length === 0
  );
}

const isRecord = (value: unknown): value is Record<string, unknown> =>
  typeof value === 'object' && value !== null && !Array.isArray(value);

/** `GET /api/auth/reauth-status` if it has the shape of the protocol; `null` otherwise. */
export function parseReauthStatus(value: unknown): ReauthStatus | null {
  if (!isRecord(value) || typeof value.link_ready !== 'boolean') return null;
  if (!Array.isArray(value.unlink)) return null;
  const valid = value.unlink.every(
    (entry: unknown) =>
      isRecord(entry) &&
      isProvider(entry.provider) &&
      typeof entry.ready === 'boolean' &&
      Array.isArray(entry.reauth_with) &&
      entry.reauth_with.every(isProvider),
  );
  return valid ? (value as unknown as ReauthStatus) : null;
}
