export function formatTimestamp(value?: string | null): string {
  if (!value) {
    return "—";
  }
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return value;
  }
  return date.toLocaleString();
}

export function formatUnixMs(value?: number | null): string {
  if (value === null || value === undefined) {
    return "—";
  }
  return new Date(value).toLocaleString();
}
