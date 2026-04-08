export type UpdateSourceMode = "github" | "web" | "custom";
export type UpdateChannel = "release" | "staging";

export function normalizeUpdateSourceMode(value: string | null | undefined): UpdateSourceMode {
  const normalized = (value ?? "").trim().toLowerCase();
  if (normalized === "github" || normalized === "") {
    return "github";
  }
  if (normalized === "web") {
    return "web";
  }
  return "custom";
}

export function normalizeUpdateChannel(value: string | null | undefined): UpdateChannel {
  const normalized = (value ?? "").trim().toLowerCase();
  return normalized === "staging" ? "staging" : "release";
}

export function resolveUpdateSourceValue(mode: UpdateSourceMode, customValue: string): string {
  if (mode === "custom") {
    return customValue.trim();
  }
  return mode;
}

export function resolveUpdateTag(channel: UpdateChannel): string | null {
  return channel === "staging" ? "staging" : null;
}
