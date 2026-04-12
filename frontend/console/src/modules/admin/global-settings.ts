export type UpdateChannel = "release" | "staging";

export function normalizeUpdateChannel(value: string | null | undefined): UpdateChannel {
  const normalized = (value ?? "").trim().toLowerCase();
  return normalized === "staging" ? "staging" : "release";
}

export function resolveUpdateTag(channel: UpdateChannel): string | null {
  return channel === "staging" ? "staging" : null;
}
