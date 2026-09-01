import type { TokenizerVocabDto } from "@/generated/TokenizerVocabDto"
import type { TokenizerDownloadProgressDto } from "@/generated/TokenizerDownloadProgressDto"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { Trash2Icon } from "lucide-react"
import { useState } from "react"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import { deleteTokenizerVocab, fetchTokenizerVocab, tokenizerVocabProgress } from "@/api/control"
import { DataTable, type DataTableColumn } from "@/components/data-table"
import { Button } from "@/components/ui/button"
import { Field, FieldDescription, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Progress } from "@/components/ui/progress"
import { formatByteSize, formatInstant, formatNumber, formatPercent } from "@/lib/format"

type Props = { values: Array<TokenizerVocabDto> }

export function TokenizerVocabsCard({ values }: Props) {
  const { t, i18n } = useTranslation()
  const queryClient = useQueryClient()
  const [name, setName] = useState("")
  const [repository, setRepository] = useState("")
  const [fetchingName, setFetchingName] = useState<string | null>(null)
  const refresh = async () => queryClient.invalidateQueries({ queryKey: ["tokenizer-vocabs"] })
  const fetchVocab = useMutation({
    mutationFn: fetchTokenizerVocab,
    onSuccess: async () => { setName(""); setRepository(""); await refresh(); toast.success(t("settings.tokenizers.fetched")) },
    onError: () => toast.error(t("settings.tokenizers.fetchError")),
    onSettled: () => setFetchingName(null),
  })
  const progress = useQuery({
    queryKey: ["tokenizer-vocab-progress", fetchingName],
    queryFn: () => tokenizerVocabProgress(fetchingName ?? ""),
    enabled: fetchingName !== null,
    refetchInterval: 250,
  })
  const remove = useMutation({
    mutationFn: deleteTokenizerVocab,
    onSuccess: async () => { await refresh(); toast.success(t("settings.tokenizers.deleted")) },
    onError: () => toast.error(t("settings.tokenizers.deleteError")),
  })
  const actions = (vocab: TokenizerVocabDto) => (
    <Button size="icon-sm" variant="ghost" disabled={remove.isPending} aria-label={`${t("common.actions.delete")}: ${vocab.name}`} onClick={() => remove.mutate({ name: vocab.name })}>
      <Trash2Icon aria-hidden />
    </Button>
  )
  const columns: Array<DataTableColumn<TokenizerVocabDto>> = [
    { key: "name", label: t("common.name"), header: t("common.name"), cell: (vocab) => <span className="font-mono text-xs">{vocab.name}</span> },
    { key: "repository", label: t("settings.tokenizers.repository"), header: t("settings.tokenizers.repository"), cell: (vocab) => <span className="font-mono text-xs">{vocab.repository}</span> },
    { key: "size", label: t("settings.tokenizers.size"), header: t("settings.tokenizers.size"), cell: (vocab) => t("settings.tokenizers.bytes", { value: formatNumber(vocab.size_bytes, i18n.language) }) },
    { key: "updated", label: t("settings.tokenizers.updated"), header: t("settings.tokenizers.updated"), cell: (vocab) => formatInstant(vocab.updated_at, i18n.language) },
    { key: "actions", label: t("common.actions.delete"), header: <span className="sr-only">{t("common.actions.delete")}</span>, cell: actions, className: "text-right" },
  ]

  return (
    <div className="flex flex-col gap-4">
      <form className="flex flex-col gap-3" onSubmit={(event) => { event.preventDefault(); const localName = name.trim(); const source = repository.trim(); if (!localName || !source) return; setFetchingName(localName); fetchVocab.mutate({ name: localName, repository: source }) }}>
        <div className="grid gap-3 sm:grid-cols-2">
          <Field>
            <FieldLabel htmlFor="tokenizer-name">{t("settings.tokenizers.localName")}</FieldLabel>
            <Input id="tokenizer-name" className="font-mono" placeholder={t("settings.tokenizers.namePlaceholder")} value={name} onChange={(event) => setName(event.target.value)} />
          </Field>
          <Field>
            <FieldLabel htmlFor="tokenizer-repo">{t("settings.tokenizers.repository")}</FieldLabel>
            <Input id="tokenizer-repo" className="font-mono" placeholder={t("settings.tokenizers.repositoryPlaceholder")} value={repository} onChange={(event) => setRepository(event.target.value)} />
          </Field>
        </div>
        <div className="flex items-start justify-between gap-3">
          <FieldDescription>{t("settings.tokenizers.hint")}</FieldDescription>
          <Button type="submit" size="sm" disabled={!name.trim() || !repository.trim() || fetchVocab.isPending}>
            {t(fetchVocab.isPending ? "settings.tokenizers.fetching" : "settings.tokenizers.fetch")}
          </Button>
        </div>
        {fetchingName ? <DownloadProgress name={fetchingName} progress={progress.data ?? null} /> : null}
      </form>
      <DataTable columns={columns} rows={values} rowKey={(vocab) => vocab.name} searchText={(vocab) => `${vocab.name} ${vocab.repository}`} renderCard={(vocab) => <div className="flex items-center justify-between gap-3"><div className="min-w-0"><p className="truncate font-mono text-xs">{vocab.name}</p><p className="truncate font-mono text-xs text-muted-foreground">{vocab.repository}</p><p className="text-xs text-muted-foreground">{t("settings.tokenizers.bytes", { value: formatNumber(vocab.size_bytes, i18n.language) })} · {formatInstant(vocab.updated_at, i18n.language)}</p></div>{actions(vocab)}</div>} empty={t("settings.tokenizers.empty")} storageKey="tokenizer-vocabs" />
    </div>
  )
}

function DownloadProgress({ name, progress }: { name: string; progress: TokenizerDownloadProgressDto | null }) {
  const { t, i18n } = useTranslation()
  const downloaded = progress?.downloaded_bytes ?? 0
  const total = progress?.total_bytes ?? null
  const ratio = total && total > 0 ? Math.min(downloaded / total, 1) : null
  const detail = total == null || total <= 0
    ? t("settings.tokenizers.downloadProgressUnknown", { downloaded: formatByteSize(downloaded, i18n.language) })
    : t("settings.tokenizers.downloadProgressKnown", {
        downloaded: formatByteSize(downloaded, i18n.language),
        total: formatByteSize(total, i18n.language),
        percent: formatPercent(Math.min(downloaded / total, 1), i18n.language),
      })
  return (
    <div className="flex flex-col gap-2">
      <div className="flex items-center justify-between gap-3 text-xs">
        <span className="truncate font-medium">{t("settings.tokenizers.downloadProgress", { name })}</span>
        <span className="shrink-0 text-muted-foreground">{detail}</span>
      </div>
      <Progress value={ratio == null ? null : ratio * 100} aria-label={t("settings.tokenizers.downloadProgress", { name })} />
    </div>
  )
}
