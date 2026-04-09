export function parseRequiredI64(value: string, field: string): number {
  const parsed = Number(value);
  if (!Number.isInteger(parsed)) {
    throw new Error(`${field} must be an integer`);
  }
  return parsed;
}

export function parseRequiredPositiveInteger(value: string, field: string): number {
  const parsed = parseRequiredI64(value, field);
  if (parsed <= 0) {
    throw new Error(`${field} must be greater than 0`);
  }
  return parsed;
}

/// Parse an optional integer string into `number | null`. Empty string and
/// whitespace-only inputs return null. Throws on non-integer input so the
/// caller can surface a meaningful error to the user.
export function parseOptionalI64(value: string): number | null {
  const trimmed = value.trim();
  if (!trimmed) {
    return null;
  }
  const parsed = Number(trimmed);
  if (!Number.isInteger(parsed)) {
    throw new Error("must be an integer");
  }
  return parsed;
}

export function parseOptionalFloat(value: string): number | null {
  const trimmed = value.trim();
  if (!trimmed) {
    return null;
  }
  const parsed = Number(trimmed);
  if (!Number.isFinite(parsed)) {
    throw new Error("value must be a number");
  }
  return parsed;
}
