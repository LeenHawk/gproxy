//! The landing page, which is the caller's own account rather than an operator
//! dashboard.
//!
//! Most callers are not operators, and the console's root belongs to the
//! majority. What it answers is the three questions somebody actually arrives
//! with: what am I subscribed to, how much have I got left, and what has it
//! cost me this week.

import { useQuery } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import * as portal from "@/api/portal"
import { useConsoleContext } from "@/capability/session"
import { Page, PageHeader, PageSection } from "@/components/page"
import { QuotaWindows } from "@/components/quota-windows"
import { QueryState } from "@/components/state"
import { UsageSummary } from "@/components/usage-summary"
import { Badge } from "@/components/ui/badge"
import { Link } from "@/lib/router"
import { formatInstant } from "@/lib/format"

const WEEK = 604_800_000

function SubscriptionCard() {
  const { t, i18n } = useTranslation()
  const context = useConsoleContext()
  const subscription = context.subscription
  if (!subscription) {
    return <p className="text-sm text-muted-foreground">{t("overview.noSubscription")}</p>
  }
  return (
    <div className="flex flex-wrap items-center gap-3 rounded-lg border border-border p-3 text-sm">
      <span className="font-medium">{subscription.planName ?? subscription.planId}</span>
      <Badge variant={subscription.enabled ? "success" : "outline"}>
        {subscription.enabled ? t("values.yes") : t("values.no")}
      </Badge>
      <span className="text-muted-foreground">
        {subscription.expiresAtMs === null
          ? t("overview.noExpiry")
          : t("overview.expires", { at: formatInstant(subscription.expiresAtMs, i18n.language) })}
      </span>
    </div>
  )
}

export function OverviewPage() {
  const { t } = useTranslation()
  const context = useConsoleContext()
  const quota = useQuery({ queryKey: ["portal", "quota"], queryFn: portal.quota })
  // The window is computed inside the query function rather than during the
  // render: the clock is not a pure value, and a millisecond in a query key
  // would make every render a fresh cache entry.
  const usage = useQuery({
    queryKey: ["portal", "usage", "week"],
    queryFn: () => {
      const now = Date.now()
      return portal.usage({ fromMs: now - WEEK, toMs: now })
    },
  })

  return (
    <Page>
      <PageHeader title={t("overview.title", { name: context.userName })} description={t("description.overview")} />

      <PageSection title={t("overview.subscription")}>
        <SubscriptionCard />
      </PageSection>

      <PageSection
        title={t("overview.usage")}
        description={t("overview.usageDescription")}
        actions={<Link to="/usage" className="text-sm underline underline-offset-4">{t("actions.details")}</Link>}
      >
        <QueryState isPending={usage.isPending} error={usage.error} rows={2}>
          {usage.data ? <UsageSummary summary={usage.data.summary} /> : null}
        </QueryState>
      </PageSection>

      <PageSection
        title={t("overview.quota")}
        actions={<Link to="/quota" className="text-sm underline underline-offset-4">{t("actions.details")}</Link>}
      >
        <QueryState isPending={quota.isPending} error={quota.error} rows={2}>
          <QuotaWindows windows={quota.data ?? []} />
        </QueryState>
      </PageSection>
    </Page>
  )
}
