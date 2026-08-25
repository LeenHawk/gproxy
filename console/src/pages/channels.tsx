import { useQuery } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { channels } from "@/api/observability"
import { ChannelCatalog } from "@/components/channels/channel-catalog"
import { PageLayout } from "@/components/page-layout"
import { QueryState } from "@/components/query-state"

export function ChannelsPage() {
  const { t } = useTranslation()
  const query = useQuery({ queryKey: ["channels"], queryFn: channels })
  return (
    <PageLayout title={t("nav.channels")} description={t("channels.description")}>
      <QueryState loading={query.isLoading} error={query.error ? t("common.loadError") : ""} empty={query.data?.length === 0 ? t("channels.empty") : undefined}>
        <ChannelCatalog channels={query.data ?? []} />
      </QueryState>
    </PageLayout>
  )
}
