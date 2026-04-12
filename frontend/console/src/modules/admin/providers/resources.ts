import type { MemoryModelRow, ProviderRow } from "../../../lib/types/admin";

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
  return rows.filter((row) => row.provider_id === providerId && row.alias_of == null);
}

export type DerivedAliasRow = {
  id: number;
  alias: string;
  provider_name: string;
  model_id: string;
  provider_id: number;
};

export function filterAliasesForProvider(
  allModels: MemoryModelRow[],
  providerId: number | null,
  providers: ProviderRow[],
): DerivedAliasRow[] {
  if (providerId == null) {
    return [];
  }
  const aliasModels = allModels.filter((m) => m.alias_of != null && m.provider_id === providerId);
  return aliasModels
    .map((alias) => {
      const target = allModels.find((m) => m.id === alias.alias_of);
      if (!target) return null;
      const provider = providers.find((p) => p.id === target.provider_id);
      return {
        id: alias.id,
        alias: alias.model_id,
        provider_name: provider?.name ?? String(target.provider_id),
        model_id: target.model_id,
        provider_id: target.provider_id,
      };
    })
    .filter(Boolean) as DerivedAliasRow[];
}

export function providerOptionLabel(provider: ProviderRow): string {
  return `${provider.name} (#${provider.id})`;
}
