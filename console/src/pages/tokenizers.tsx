import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { useState } from "react"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import { instanceSettings, saveInstanceSettings, tokenizerVocabs } from "@/api/control"
import type { InstanceSettingsDto } from "@/generated/InstanceSettingsDto"
import { PageLayout } from "@/components/page-layout"
import { QueryState } from "@/components/query-state"
import { TokenizerVocabsCard } from "@/components/settings/tokenizer-vocabs-card"
import { Field, FieldContent, FieldDescription, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Switch } from "@/components/ui/switch"

const BUILTIN_DEFAULT_VOCAB = "deepseek-v4-pro"

export function TokenizersPage() {
  const { t } = useTranslation()
  const client = useQueryClient()
  const settings = useQuery({ queryKey: ["instance-settings"], queryFn: instanceSettings })
  const vocabs = useQuery({ queryKey: ["tokenizer-vocabs"], queryFn: tokenizerVocabs })
  const save = useMutation({
    mutationFn: (value: InstanceSettingsDto) => saveInstanceSettings(value),
    onSuccess: async () => { await client.invalidateQueries({ queryKey: ["instance-settings"] }) },
    onError: () => toast.error(t("settings.saveError")),
  })
  const current = settings.data
  return (
    <PageLayout title={t("settings.tokenizers.title")} description={t("settings.tokenizers.description")}>
      <div className="flex max-w-4xl flex-col gap-8">
        <QueryState loading={settings.isLoading || vocabs.isLoading} error={settings.error || vocabs.error ? t("settings.tokenizers.loadError") : ""}>
          {current && vocabs.data ? <>
            <Field orientation="horizontal">
              <FieldContent>
                <FieldLabel htmlFor="tokenizer-vocabs">{t("settings.runtime.enable_tokenizer_vocabs")}</FieldLabel>
                <FieldDescription>{t("settings.runtime.enable_tokenizer_vocabsHint")}</FieldDescription>
              </FieldContent>
              <Switch
                id="tokenizer-vocabs"
                checked={current.enable_tokenizer_vocabs}
                disabled={save.isPending}
                onCheckedChange={(value) => save.mutate({ ...current, enable_tokenizer_vocabs: value })}
              />
            </Field>
            <Field orientation="horizontal">
              <FieldContent>
                <FieldLabel htmlFor="tokenizer-download">{t("settings.runtime.enable_tokenizer_download")}</FieldLabel>
                <FieldDescription>{t("settings.runtime.enable_tokenizer_downloadHint")}</FieldDescription>
              </FieldContent>
              <Switch
                id="tokenizer-download"
                checked={current.enable_tokenizer_download}
                disabled={save.isPending || !current.enable_tokenizer_vocabs}
                onCheckedChange={(value) => save.mutate({ ...current, enable_tokenizer_download: value })}
              />
            </Field>
            <DefaultVocabField key={current.default_tokenizer_vocab ?? ""} settings={current} saving={save.isPending} onSave={(value) => save.mutate(value)} />
            <TokenizerVocabsCard values={vocabs.data} />
          </> : null}
        </QueryState>
      </div>
    </PageLayout>
  )
}

function DefaultVocabField({ settings, saving, onSave }: { settings: InstanceSettingsDto; saving: boolean; onSave: (value: InstanceSettingsDto) => void }) {
  const { t } = useTranslation()
  const stored = settings.default_tokenizer_vocab ?? BUILTIN_DEFAULT_VOCAB
  const [draft, setDraft] = useState(stored)
  const commit = () => {
    const next = draft.trim()
    if (next === stored) return
    onSave({ ...settings, default_tokenizer_vocab: next || null })
  }
  return (
    <Field>
      <FieldLabel htmlFor="default-vocab">{t("settings.runtime.default_tokenizer_vocab")}</FieldLabel>
      <Input
        id="default-vocab"
        className="font-mono"
        placeholder={t("settings.tokenizers.placeholder")}
        value={draft}
        disabled={saving || !settings.enable_tokenizer_vocabs}
        onChange={(event) => setDraft(event.target.value)}
        onBlur={commit}
        onKeyDown={(event) => { if (event.key === "Enter") { event.preventDefault(); commit() } }}
      />
      <FieldDescription>{t("settings.runtime.default_tokenizer_vocabHint")}</FieldDescription>
    </Field>
  )
}
