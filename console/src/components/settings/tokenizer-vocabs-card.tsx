import type { TokenizerVocabDto } from "@/generated/TokenizerVocabDto"
import { useMutation, useQueryClient } from "@tanstack/react-query"
import { Trash2Icon } from "lucide-react"
import { useState } from "react"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import { deleteTokenizerVocab, fetchTokenizerVocab } from "@/api/control"
import { DataTable, type DataTableColumn } from "@/components/data-table"
import { Button } from "@/components/ui/button"
import { Field, FieldDescription, FieldLabel } from "@/components/ui/field"
import { InputGroup, InputGroupAddon, InputGroupButton, InputGroupInput } from "@/components/ui/input-group"
import { formatInstant, formatNumber } from "@/lib/format"

type Props = { values: Array<TokenizerVocabDto> }

export function TokenizerVocabsCard({ values }: Props) {
  const { t, i18n } = useTranslation()
  const queryClient = useQueryClient()
  const [name, setName] = useState("")
  const refresh = async () => queryClient.invalidateQueries({ queryKey: ["tokenizer-vocabs"] })
  const fetchVocab = useMutation({
    mutationFn: fetchTokenizerVocab,
    onSuccess: async () => { setName(""); await refresh(); toast.success(t("settings.tokenizers.fetched")) },
    onError: () => toast.error(t("settings.tokenizers.fetchError")),
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
    { key: "size", label: t("settings.tokenizers.size"), header: t("settings.tokenizers.size"), cell: (vocab) => t("settings.tokenizers.bytes", { value: formatNumber(vocab.size_bytes, i18n.language) }) },
    { key: "updated", label: t("settings.tokenizers.updated"), header: t("settings.tokenizers.updated"), cell: (vocab) => formatInstant(vocab.updated_at, i18n.language) },
    { key: "actions", label: t("common.actions.delete"), header: <span className="sr-only">{t("common.actions.delete")}</span>, cell: actions, className: "text-right" },
  ]

  return (
    <div className="flex flex-col gap-4">
      <form onSubmit={(event) => { event.preventDefault(); fetchVocab.mutate({ name: name.trim() }) }}>
        <Field>
          <FieldLabel htmlFor="tokenizer-repo">{t("settings.tokenizers.repository")}</FieldLabel>
          <InputGroup>
            <InputGroupInput id="tokenizer-repo" className="font-mono" placeholder={t("settings.tokenizers.placeholder")} value={name} onChange={(event) => setName(event.target.value)} />
            <InputGroupAddon align="inline-end">
              <InputGroupButton type="submit" variant="default" size="sm" disabled={!name.trim() || fetchVocab.isPending}>
                {t(fetchVocab.isPending ? "settings.tokenizers.fetching" : "settings.tokenizers.fetch")}
              </InputGroupButton>
            </InputGroupAddon>
          </InputGroup>
          <FieldDescription>{t("settings.tokenizers.hint")}</FieldDescription>
        </Field>
      </form>
      <DataTable columns={columns} rows={values} rowKey={(vocab) => vocab.name} searchText={(vocab) => vocab.name} renderCard={(vocab) => <div className="flex items-center justify-between gap-3"><div className="min-w-0"><p className="truncate font-mono text-xs">{vocab.name}</p><p className="text-xs text-muted-foreground">{t("settings.tokenizers.bytes", { value: formatNumber(vocab.size_bytes, i18n.language) })} · {formatInstant(vocab.updated_at, i18n.language)}</p></div>{actions(vocab)}</div>} empty={t("settings.tokenizers.empty")} storageKey="tokenizer-vocabs" />
    </div>
  )
}
