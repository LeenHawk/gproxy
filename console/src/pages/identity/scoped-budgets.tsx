import { useQuery } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { api } from "@/api/client"
import type { CredentialOwnerOptionDto } from "@/generated/app"
import { QuotaButton } from "@/pages/quotas"
import { Page, PageHeader } from "@/components/page"
import { DataTable } from "@/components/data-table"
import { EmptyNotice, QueryState } from "@/components/state"

/** Budget access does not grant identity CRUD: only the admitted owner directory is read. */
export function ScopedBudgetObjects({ kind }: { kind: "org" | "team" }) {
  const { t } = useTranslation()
  const owners = useQuery({ queryKey: ["credential-owners"], queryFn: () => api<CredentialOwnerOptionDto[]>("/admin/api/credentials/owners") })
  return <Page><PageHeader title={t(kind === "org" ? "nav.organizations" : "nav.teams")} /><QueryState isPending={owners.isPending} error={owners.error}>
    <DataTable rows={owners.data?.filter(owner => owner.kind === kind) ?? []} rowKey={row => row.id} empty={<EmptyNotice title={t("state.emptyTitle")} />} columns={[{ key: "name", cell: row => row.name }]} actions={row => <QuotaButton ownerKind={row.kind} ownerId={row.id} name={row.name} />} />
  </QueryState></Page>
}
