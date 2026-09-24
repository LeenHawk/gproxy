import { family, type Family } from "@/api/admin"
import { api, json } from "@/api/client"
import type { BatchItem } from "@/generated/sdk"
export type ConfigFamily<D, W, P> = Family<D, W, P> & { batch: (items: BatchItem<W, P>[]) => Promise<(D | null)[]> }
export function configFamily<D, W, P>(path: string): ConfigFamily<D, W, P> {
  return { ...family<D, W, P>(path), batch: items => api<(D | null)[]>(`/admin/api${path}/batch`, json("POST", items)) }
}
