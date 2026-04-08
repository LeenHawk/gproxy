import type { MemoryModelAliasRow, MemoryModelRow, ProviderRow } from "../../../lib/types/admin";

export function nextResourceId<T extends { id: number }>(rows: T[]): string {
  return String(rows.reduce((max, row) => Math.max(max, row.id), 0) + 1);
}

export function filterModelsForProvider(
  rows: MemoryModelRow[],
  providerId: number | null,
): MemoryModelRow[] {
  if (providerId === null) {
    return [];
  }
  return rows.filter((row) => row.provider_id === providerId);
}

export function filterAliasesForProvider(
  rows: MemoryModelAliasRow[],
  providerName: string | null,
): MemoryModelAliasRow[] {
  if (!providerName) {
    return [];
  }
  return rows.filter((row) => row.provider_name === providerName);
}

export function providerOptionLabel(provider: ProviderRow): string {
  return `${provider.name} (#${provider.id})`;
}
