//! The caller's own budget windows.

import { useQuery } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import * as portal from "@/api/portal"
import { Page, PageHeader } from "@/components/page"
import { QuotaWindows } from "@/components/quota-windows"
import { QueryState } from "@/components/state"

export function QuotaPage() {
  const { t } = useTranslation()
  const windows = useQuery({ queryKey: ["portal", "quota"], queryFn: portal.quota })
  return (
    <Page>
      <PageHeader title={t("nav.quota")} description={t("description.quota")} />
      <QueryState isPending={windows.isPending} error={windows.error}>
        <QuotaWindows windows={windows.data ?? []} />
      </QueryState>
    </Page>
  )
}
