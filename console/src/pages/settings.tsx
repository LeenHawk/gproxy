import { useQuery } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { instanceSettings, tokenizerVocabs } from "@/api/control"
import { PageLayout } from "@/components/page-layout"
import { QueryState } from "@/components/query-state"
import { InstanceSettingsForm } from "@/components/settings/instance-settings-form"
import { TokenizerVocabsCard } from "@/components/settings/tokenizer-vocabs-card"
import { ConfigurationTransferCard } from "@/components/settings/configuration-transfer-card"
import { AutostartCard } from "@/components/settings/autostart-card"

export function SettingsPage() {
  const { t } = useTranslation()
  const query = useQuery({ queryKey: ["instance-settings"], queryFn: instanceSettings })
  const vocabs = useQuery({ queryKey: ["tokenizer-vocabs"], queryFn: tokenizerVocabs })
  return (
    <PageLayout title={t("settings.title")} description={t("settings.subtitle")}>
      <QueryState loading={query.isLoading} error={query.error ? t("settings.loadError") : ""}>
        {query.data ? <InstanceSettingsForm settings={query.data} /> : null}
      </QueryState>
      <ConfigurationTransferCard />
      <AutostartCard />
      <QueryState loading={vocabs.isLoading} error={vocabs.error ? t("settings.tokenizers.loadError") : ""}>
        {query.data && vocabs.data ? <TokenizerVocabsCard values={vocabs.data} downloadEnabled={query.data.enable_tokenizer_download} /> : null}
      </QueryState>
    </PageLayout>
  )
}
