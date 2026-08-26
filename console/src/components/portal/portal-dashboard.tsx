import { useState } from "react"
import { useTranslation } from "react-i18next"
import type { PortalContextDto } from "@/generated/PortalContextDto"
import type { PortalModelDto } from "@/generated/PortalModelDto"
import type { PortalQuotaWindowDto } from "@/generated/PortalQuotaWindowDto"
import type { PortalRecentRequestDto } from "@/generated/PortalRecentRequestDto"
import type { PortalUsageDto } from "@/generated/PortalUsageDto"
import { ConnectionGuide } from "@/components/portal/connection-guide"
import { ModelCatalog } from "@/components/portal/model-catalog"
import { QuotaWindows } from "@/components/portal/quota-windows"
import { RecentRequests } from "@/components/portal/recent-requests"
import { UsagePanel, type UsageDays } from "@/components/portal/usage-panel"
import { QueryState } from "@/components/query-state"

export function PortalDashboard({
  context,
  apiKey,
  origin,
  models,
  modelsLoading,
  modelsError,
  usage,
  usageDays,
  usageLoading,
  usageError,
  quotaWindows,
  quotaLoading,
  quotaError,
  recentRequests,
  recentLoading,
  recentError,
  onUsageDaysChange,
}: {
  context: PortalContextDto
  apiKey: string
  origin: string
  models: Array<PortalModelDto>
  modelsLoading: boolean
  modelsError: boolean
  usage: PortalUsageDto | undefined
  usageDays: UsageDays
  usageLoading: boolean
  usageError: boolean
  quotaWindows: Array<PortalQuotaWindowDto>
  quotaLoading: boolean
  quotaError: boolean
  recentRequests: Array<PortalRecentRequestDto>
  recentLoading: boolean
  recentError: boolean
  onUsageDaysChange: (days: UsageDays) => void
}) {
  const { t } = useTranslation()
  const [requestedModel, setRequestedModel] = useState<string | null>(null)
  const selectedModel = models.some((model) => model.name === requestedModel)
    ? requestedModel
    : models[0]?.name ?? null

  return (
    <>
      <section className="flex flex-col gap-2">
        <h1 className="text-2xl font-semibold tracking-tight">{t("portal.overview.title", { name: context.user_name })}</h1>
        <p className="max-w-3xl text-sm leading-6 text-muted-foreground">{t("portal.overview.description")}</p>
      </section>
      <QueryState loading={modelsLoading} error={modelsError ? t("portal.models.loadError") : ""}>
        <div className="flex flex-col gap-6">
          <ConnectionGuide
            origin={origin}
            apiKey={apiKey}
            models={models}
            selectedModel={selectedModel}
            onModelChange={setRequestedModel}
          />
          <ModelCatalog models={models} />
        </div>
      </QueryState>
      <div className="grid gap-6 lg:grid-cols-2">
        <UsagePanel
          usage={usage}
          days={usageDays}
          loading={usageLoading}
          error={usageError}
          onDaysChange={onUsageDaysChange}
        />
        <QuotaWindows windows={quotaWindows} loading={quotaLoading} error={quotaError} />
      </div>
      {context.recent_requests_enabled ? (
        <RecentRequests requests={recentRequests} loading={recentLoading} error={recentError} />
      ) : null}
    </>
  )
}
