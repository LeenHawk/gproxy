import { useQuery } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { instanceSettings, tokenizerVocabs } from "@/api/control"
import { PageLayout } from "@/components/page-layout"
import { QueryState } from "@/components/query-state"
import { TokenizerVocabsCard } from "@/components/settings/tokenizer-vocabs-card"

export function TokenizersPage() {
  const { t } = useTranslation()
  const settings = useQuery({ queryKey: ["instance-settings"], queryFn: instanceSettings })
  const vocabs = useQuery({ queryKey: ["tokenizer-vocabs"], queryFn: tokenizerVocabs })
  return (
    <PageLayout title={t("settings.tokenizers.title")} description={t("settings.tokenizers.description")}>
      <QueryState loading={settings.isLoading || vocabs.isLoading} error={settings.error || vocabs.error ? t("settings.tokenizers.loadError") : ""}>
        {settings.data && vocabs.data ? <TokenizerVocabsCard values={vocabs.data} downloadEnabled={settings.data.enable_tokenizer_download} /> : null}
      </QueryState>
    </PageLayout>
  )
}
