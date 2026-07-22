import {
  SUFFIX_PROTOCOL_LABELS,
  UPSTREAM_SOURCE_GROUP_BY_CHANNEL,
  suffixGroupsForChannel,
  suffixProtocolForChannel,
  type SuffixAction,
  type SuffixProtocol,
} from "./index";

export interface SuffixSelection {
  protocol: SuffixProtocol;
  picks: Record<string, string>;
  upstream: string;
  preservedActions: SuffixAction[];
}

function valuesEqual(a: unknown, b: unknown): boolean {
  if (Object.is(a, b)) return true;
  if (Array.isArray(a) || Array.isArray(b)) {
    return Array.isArray(a)
      && Array.isArray(b)
      && a.length === b.length
      && a.every((value, i) => valuesEqual(value, b[i]));
  }
  if (!a || !b || typeof a !== "object" || typeof b !== "object") return false;
  const aRecord = a as Record<string, unknown>;
  const bRecord = b as Record<string, unknown>;
  const keys = Object.keys(aRecord);
  return keys.length === Object.keys(bRecord).length
    && keys.every((key) => Object.hasOwn(bRecord, key) && valuesEqual(aRecord[key], bRecord[key]));
}

function matchActions(
  actual: SuffixAction[],
  expected: SuffixAction[],
  used: Set<number>,
): number[] | null {
  const matched: number[] = [];
  for (const action of expected) {
    const index = actual.findIndex((candidate, i) =>
      !used.has(i)
      && !matched.includes(i)
      && candidate.path === action.path
      && valuesEqual(candidate.value, action.value));
    if (index < 0) return null;
    matched.push(index);
  }
  return matched;
}

function selectionForProtocol(
  protocol: SuffixProtocol,
  channel: string,
  actions: SuffixAction[],
): SuffixSelection & { matched: number } {
  const picks: Record<string, string> = {};
  const used = new Set<number>();
  const sourceGroup = UPSTREAM_SOURCE_GROUP_BY_CHANNEL[channel];

  for (const group of suffixGroupsForChannel(protocol, channel)) {
    const index = group.entries.findIndex((entry) => {
      const matched = matchActions(actions, entry.actions, used);
      if (!matched) return false;
      matched.forEach((i) => used.add(i));
      return true;
    });
    if (index >= 0) picks[group.key] = String(index);
  }

  const upstreamPath = channel === "openrouter"
    ? "provider.only"
    : channel === "vercel"
      ? "providerOptions.gateway.only"
      : null;
  let upstream = "";
  if (upstreamPath && (!sourceGroup || picks[sourceGroup.key] == null)) {
    const index = actions.findIndex((action, i) =>
      !used.has(i)
      && action.path === upstreamPath
      && Array.isArray(action.value)
      && action.value.every((value) => typeof value === "string"));
    if (index >= 0) {
      upstream = (actions[index]?.value as string[]).join(", ");
      used.add(index);
    }
  }

  return {
    protocol,
    picks,
    upstream,
    preservedActions: actions.filter((_, i) => !used.has(i)),
    matched: used.size,
  };
}

/** Infer picker controls from persisted actions, preserving anything not in the catalog. */
export function inferSuffixSelection(channel: string, actions: SuffixAction[]): SuffixSelection {
  const fallback = suffixProtocolForChannel(channel);
  const protocols = [
    fallback,
    ...(Object.keys(SUFFIX_PROTOCOL_LABELS) as SuffixProtocol[]).filter((p) => p !== fallback),
  ];
  let best = selectionForProtocol(protocols[0], channel, actions);
  for (const protocol of protocols.slice(1)) {
    const candidate = selectionForProtocol(protocol, channel, actions);
    if (candidate.matched > best.matched) best = candidate;
  }
  return {
    protocol: best.protocol,
    picks: best.picks,
    upstream: best.upstream,
    preservedActions: best.preservedActions,
  };
}
