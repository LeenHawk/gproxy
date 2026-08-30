import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import { instanceSettings, saveInstanceSettings, tokenizerVocabs } from "@/api/control"
import type { InstanceSettingsDto } from "@/generated/InstanceSettingsDto"
import { PageLayout } from "@/components/page-layout"
import { QueryState } from "@/components/query-state"
import { TokenizerVocabsCard } from "@/components/settings/tokenizer-vocabs-card"
import { Field, FieldContent, FieldDescription, FieldLabel } from "@/components/ui/field"
import { Switch } from "@/components/ui/switch"

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
                <FieldLabel htmlFor="tokenizer-download">{t("settings.runtime.enable_tokenizer_download")}</FieldLabel>
                <FieldDescription>{t("settings.runtime.enable_tokenizer_downloadHint")}</FieldDescription>
              </FieldContent>
              <Switch
                id="tokenizer-download"
                checked={current.enable_tokenizer_download}
                disabled={save.isPending}
                onCheckedChange={(value) => save.mutate({ ...current, enable_tokenizer_download: value })}
              />
            </Field>
            <TokenizerVocabsCard values={vocabs.data} downloadEnabled={current.enable_tokenizer_download} />
          </> : null}
        </QueryState>
      </div>
    </PageLayout>
  )
}
