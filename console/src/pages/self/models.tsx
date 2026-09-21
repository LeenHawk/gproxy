//! The model names this caller may address, and which of them they may call.
//!
//! `providerCount` and `channelIds` are instance configuration and say nothing
//! about another tenant; `permitted` is this caller's own answer, from their
//! permission rules. Both are rendered, because "the name exists but is not
//! yours" and "the name does not exist" are different problems.

import { useQuery } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import * as portal from "@/api/portal"
import { BoolCell } from "@/components/cells"
import { DataTable, IdCell } from "@/components/data-table"
import { Page, PageHeader } from "@/components/page"
import { EmptyNotice, QueryState } from "@/components/state"
import { Badge } from "@/components/ui/badge"

export function ModelsPage() {
  const { t } = useTranslation()
  const list = useQuery({ queryKey: ["portal", "models"], queryFn: portal.models })
  return (
    <Page>
      <PageHeader title={t("nav.models")} description={t("description.models")} />
      <QueryState isPending={list.isPending} error={list.error}>
        <DataTable
          columns={[
            { key: "name", cell: (row) => <IdCell value={row.name} className="text-sm text-foreground" /> },
            {
              key: "channelIds",
              cell: (row) => (
                <span className="flex flex-wrap gap-1">
                  {row.channelIds.map((channel) => <Badge key={channel} variant="outline">{channel}</Badge>)}
                </span>
              ),
            },
            { key: "providerCount", cell: (row) => row.providerCount },
            { key: "permitted", cell: (row) => <BoolCell value={row.permitted} /> },
          ]}
          rows={list.data ?? []}
          rowKey={(row) => row.name}
          empty={<EmptyNotice title={t("models.emptyTitle")} description={t("models.emptyDescription")} />}
        />
      </QueryState>
    </Page>
  )
}
