export function formatTimestamp(value?: number | null): string {
  if (!value || !Number.isFinite(value)) {
    return "-";
  }
  const date = new Date(value * 1000);
  if (Number.isNaN(date.getTime())) {
    return "-";
  }
  return date.toLocaleString();
}

export function formatOptional(value?: string | number | null): string {
  if (value === null || value === undefined || value === "") {
    return "-";
  }
  return String(value);
}

export function toEpochSeconds(input: string): number | null {
  if (!input.trim()) {
    return null;
  }
  const date = new Date(input);
  if (Number.isNaN(date.getTime())) {
    return null;
  }
  return Math.floor(date.getTime() / 1000);
}

export function fromEpochSeconds(value?: number | null): string {
  if (!value || !Number.isFinite(value)) {
    return "";
  }
  const date = new Date(value * 1000);
  if (Number.isNaN(date.getTime())) {
    return "";
  }
  const pad = (num: number) => String(num).padStart(2, "0");
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}T${pad(
    date.getHours()
  )}:${pad(date.getMinutes())}`;
}

export function maskValue(value: string, head = 6, tail = 4): string {
  if (!value) {
    return "";
  }
  if (value.length <= head + tail + 3) {
    return value;
  }
  return `${value.slice(0, head)}...${value.slice(-tail)}`;
}
