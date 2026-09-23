//! The landing page, which is the caller's own account rather than an operator
//! dashboard.
//!
//! Most callers are not operators, and the console's root belongs to the
//! majority. What it answers is the questions somebody actually arrives
//! with: how much have I got left, and what has it cost me this week.

import { useQuery } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import * as portal from "@/api/portal"
import { useConsoleContext } from "@/capability/session"
import { Page, PageHeader, PageSection } from "@/components/page"
import { QuotaWindows } from "@/components/quota-windows"
import { QueryState } from "@/components/state"
import { UsageSummary } from "@/components/usage-summary"
import { Link } from "@/lib/router"

const WEEK = 604_800_000

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
      <PageHeader title={t("overview.title", { name: context.userName })} />

      <PageSection
        title={t("overview.usage")}
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
