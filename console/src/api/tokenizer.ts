import { api, json } from "@/api/client"
import type { ModelDto, Page, TokenizerFetch, TokenizerProgressDto, VocabularyDto } from "@/generated/sdk"

export const VOCABULARIES_KEY = ["configuration", "vocabularies"] as const
export const MODEL_CATALOG_KEY = ["configuration", "model-catalog"] as const
export const downloadVocabulary = (request: TokenizerFetch) =>
  api<VocabularyDto>("/admin/api/tokenizer-vocabs", json("POST", request))
export const vocabularyProgress = () =>
  api<TokenizerProgressDto | null>("/admin/api/tokenizer-vocabs/progress")
export const deleteVocabulary = (id: string) =>
  api<void>(`/admin/api/tokenizer-vocabs/${encodeURIComponent(id)}`, { method: "DELETE" })
export async function modelCatalog() {
  const rows: Array<ModelDto> = []
  for (let page = 1; ; page += 1) {
    const result = await api<Page<ModelDto>>(`/admin/api/models?page=${page}&pageSize=500`)
    rows.push(...result.items)
    if (rows.length >= result.total || result.items.length === 0) return rows
  }
}
export const bindVocabulary = (modelId: string, fileId: string | null) =>
  api<ModelDto>(
    `/admin/api/models/${encodeURIComponent(modelId)}`,
    json("PATCH", { vocabularyFileId: fileId }),
  )
