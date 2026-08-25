import { useTranslation } from "react-i18next"
import type { PortalQuotaWindowDto } from "@/generated/PortalQuotaWindowDto"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Empty, EmptyDescription, EmptyHeader, EmptyTitle } from "@/components/ui/empty"
import { QueryState } from "@/components/query-state"
import { WindowBar } from "@/components/window-bar"

export function QuotaWindows({
  windows,
  loading,
  error,
}: {
  windows: Array<PortalQuotaWindowDto>
  loading: boolean
  error: boolean
}) {
  const { t } = useTranslation()

  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("portal.quota.title")}</CardTitle>
        <CardDescription>{t("portal.quota.description")}</CardDescription>
      </CardHeader>
      <CardContent>
        <QueryState loading={loading} error={error ? t("portal.quota.loadError") : ""}>
          {windows.length === 0 ? (
            <Empty>
              <EmptyHeader>
                <EmptyTitle>{t("portal.quota.empty")}</EmptyTitle>
                <EmptyDescription>{t("portal.quota.emptyDescription")}</EmptyDescription>
              </EmptyHeader>
            </Empty>
          ) : (
            <div className="flex flex-col gap-6">
              {windows.map((window) => (
                <WindowBar
                  key={`${window.scope}:${window.window_kind}`}
                  label={t("portal.quota.windowLabel", {
                    scope: t(`portal.quota.scopes.${window.scope}`),
                    window: t(`portal.quota.kinds.${window.window_kind}`),
                  })}
                  used={window.cost_used}
                  limit={window.cost_limit}
                  start={window.window_start}
                  end={window.reset_at}
                  started={window.started}
                />
              ))}
            </div>
          )}
        </QueryState>
      </CardContent>
    </Card>
  )
}
