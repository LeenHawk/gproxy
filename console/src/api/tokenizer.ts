import { api, json } from "@/api/client"
import type { ModelDto, TokenizerFetch, TokenizerProgressDto, VocabularyDto } from "@/generated/sdk"

export const VOCABULARIES_KEY = ["configuration", "vocabularies"] as const
export const downloadVocabulary = (request: TokenizerFetch) =>
  api<VocabularyDto>("/admin/api/tokenizer-vocabs", json("POST", request))
export const vocabularyProgress = () =>
  api<TokenizerProgressDto | null>("/admin/api/tokenizer-vocabs/progress")
export const deleteVocabulary = (id: string) =>
  api<void>(`/admin/api/tokenizer-vocabs/${encodeURIComponent(id)}`, { method: "DELETE" })
export const bindVocabulary = (modelId: string, fileId: string | null) =>
  api<ModelDto>(
    `/admin/api/models/${encodeURIComponent(modelId)}`,
    json("PATCH", { vocabularyFileId: fileId }),
  )
