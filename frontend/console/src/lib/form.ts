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
