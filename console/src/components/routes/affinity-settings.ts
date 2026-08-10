export type AffinitySubject = "user" | "conversation";

export const MAX_REANCHOR_AFTER_SECS = 31_536_000;

export interface AffinityState {
  enabled: boolean;
  subject: AffinitySubject;
  reanchorAfterSecs: string;
}

export type AffinitySettingsResult =
  | { ok: true; settings: Record<string, unknown> }
  | { ok: false; error: "reanchor_after_secs_invalid" };

function objectValue(value: unknown): Record<string, unknown> {
  return value !== null && typeof value === "object" && !Array.isArray(value)
    ? value as Record<string, unknown>
    : {};
}

export function initialAffinity(settings: unknown): AffinityState {
  const affinity = objectValue(objectValue(settings).affinity);
  const reanchor = affinity.reanchor_after_secs;
  return {
    enabled: affinity.enabled === true,
    subject: affinity.subject === "conversation" ? "conversation" : "user",
    reanchorAfterSecs: typeof reanchor === "number" || typeof reanchor === "string"
      ? String(reanchor)
      : "",
  };
}

function optionalPositiveInteger(value: string): number | null {
  if (!value.trim()) return null;
  const parsed = Number(value);
  return Number.isSafeInteger(parsed)
    && parsed > 0
    && parsed <= MAX_REANCHOR_AFTER_SECS
    ? parsed
    : NaN;
}

/** Apply the fields owned by the affinity form without dropping opaque settings. */
export function assembleAffinitySettings(
  base: unknown,
  state: AffinityState,
): AffinitySettingsResult {
  const settings = { ...objectValue(base) };
  const affinity = { ...objectValue(settings.affinity) };

  if (state.enabled) {
    const reanchorAfterSecs = optionalPositiveInteger(state.reanchorAfterSecs);
    if (Number.isNaN(reanchorAfterSecs)) {
      return { ok: false, error: "reanchor_after_secs_invalid" };
    }

    affinity.enabled = true;
    if (state.subject === "conversation") affinity.subject = "conversation";
    else delete affinity.subject;
    if (reanchorAfterSecs === null) delete affinity.reanchor_after_secs;
    else affinity.reanchor_after_secs = reanchorAfterSecs;
  } else {
    delete affinity.enabled;
    delete affinity.subject;
    delete affinity.reanchor_after_secs;
  }

  if (Object.keys(affinity).length > 0) settings.affinity = affinity;
  else delete settings.affinity;
  return { ok: true, settings };
}
