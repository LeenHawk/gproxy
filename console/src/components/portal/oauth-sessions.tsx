import { useState } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import { portalOAuthSessions, revokePortalOAuthSession } from "@/api/portal"
import type { OAuthSessionDto } from "@/generated/OAuthSessionDto"
import { ConfirmDangerous } from "@/components/confirm-dangerous"
import { DataTable, type DataTableColumn } from "@/components/data-table"
import { usePageSize } from "@/components/data-table-state"
import { QueryState } from "@/components/query-state"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"

export function OAuthSessions() {
  const { t, i18n } = useTranslation()
  const queryClient = useQueryClient()
  const [activeOnly, setActiveOnly] = useState(true)
  const [page, setPage] = useState(1)
  const [pageSize, setPageSize] = usePageSize("portal-oauth-sessions", 20)
  const [selected, setSelected] = useState<OAuthSessionDto | null>(null)
  const query = useQuery({
    queryKey: ["portal", "oauth-sessions", activeOnly, page, pageSize],
    queryFn: ({ signal }) => portalOAuthSessions(activeOnly, pageSize, (page - 1) * pageSize, signal),
    refetchInterval: 15_000,
  })
  const revoke = useMutation({
    mutationFn: revokePortalOAuthSession,
    onSuccess: async () => { setSelected(null); setPage(1); await queryClient.invalidateQueries({ queryKey: ["portal", "oauth-sessions"] }); toast.success(t("portal.oauth.revokedSuccess")) },
    onError: () => toast.error(t("portal.oauth.revokeError")),
  })
  const date = (value: number | null) => value == null ? t("portal.common.unknown") : new Date(value * 1_000).toLocaleString(i18n.language)
  const status = (session: OAuthSessionDto) => <Badge variant={session.active ? "secondary" : "outline"}>{t(session.revoked_at != null ? "portal.oauth.revoked" : session.active ? "portal.oauth.active" : "portal.oauth.expired")}</Badge>
  const action = (session: OAuthSessionDto) => <Button type="button" size="sm" variant="outline" disabled={revoke.isPending || session.revoked_at != null} onClick={() => setSelected(session)}>{t("portal.oauth.revoke")}</Button>
  const columns: Array<DataTableColumn<OAuthSessionDto>> = [
    { key: "client", label: t("portal.oauth.client"), header: t("portal.oauth.client"), cell: (session) => <div><p>{session.client_name}</p><code className="text-xs">{session.client_id}</code></div> },
    { key: "login", label: t("portal.oauth.loginTime"), header: t("portal.oauth.loginTime"), cell: (session) => date(session.logged_in_at) },
    { key: "refresh", label: t("portal.oauth.lastRefresh"), header: t("portal.oauth.lastRefresh"), cell: (session) => date(session.last_refreshed_at) },
    { key: "count", label: t("portal.oauth.refreshCount"), header: t("portal.oauth.refreshCount"), cell: (session) => session.refresh_count ?? t("portal.common.unknown") },
    { key: "expiry", label: t("portal.oauth.expires"), header: t("portal.oauth.expires"), cell: (session) => date(session.refresh_expires_at) },
    { key: "status", label: t("common.status.label"), header: t("common.status.label"), cell: status },
    { key: "actions", label: t("portal.oauth.revoke"), header: t("portal.oauth.revoke"), cell: action },
  ]
  return (
    <Card>
      <CardHeader><CardTitle>{t("portal.oauth.title")}</CardTitle><CardDescription>{t("portal.oauth.description")}</CardDescription></CardHeader>
      <CardContent className="flex flex-col gap-4">
        <dl className="grid gap-4 sm:grid-cols-2"><div><dt>{t("portal.oauth.total")}</dt><dd className="font-mono text-2xl">{query.data?.total_logins ?? "—"}</dd></div><div><dt>{t("portal.oauth.valid")}</dt><dd className="font-mono text-2xl">{query.data?.active_sessions ?? "—"}</dd></div></dl>
        <div className="flex flex-wrap justify-between gap-2">
          <ToggleGroup type="single" variant="outline" value={activeOnly ? "active" : "all"} onValueChange={(value) => { if (value) { setActiveOnly(value === "active"); setPage(1) } }} aria-label={t("portal.oauth.filter")}><ToggleGroupItem value="active">{t("portal.oauth.active")}</ToggleGroupItem><ToggleGroupItem value="all">{t("portal.oauth.all")}</ToggleGroupItem></ToggleGroup>
          <Button type="button" variant="outline" disabled={query.isFetching} onClick={() => void query.refetch()}>{t("portal.oauth.refresh")}</Button>
        </div>
        <QueryState loading={query.isLoading} error={query.isError ? t("portal.oauth.loadError") : ""}>
          <DataTable columns={columns} rows={query.data?.sessions ?? []} rowKey={(session) => session.id} searchText={(session) => session.client_name} storageKey="portal-oauth-sessions" empty={t("portal.oauth.empty")} selectable={false}
            pagination={{ page, pageSize, total: query.data?.total ?? 0, onPage: setPage, onPageSize: (size) => { setPageSize(size); setPage(1) } }}
            renderCard={(session) => <div className="flex flex-col gap-2"><p>{session.client_name}</p><code className="break-all">{session.client_id}</code>{status(session)}<p>{t("portal.oauth.loginTime")}: {date(session.logged_in_at)}</p><p>{t("portal.oauth.lastRefresh")}: {date(session.last_refreshed_at)}</p><p>{t("portal.oauth.refreshCount")}: {session.refresh_count ?? t("portal.common.unknown")}</p><p>{t("portal.oauth.expires")}: {date(session.refresh_expires_at)}</p>{action(session)}</div>} />
        </QueryState>
      </CardContent>
      <ConfirmDangerous open={selected != null} onOpenChange={(open) => { if (!open) setSelected(null) }} title={t("portal.oauth.revokeTitle")} description={t("portal.oauth.revokeDescription", { client: selected?.client_name ?? "" })} confirmLabel={t("portal.oauth.revoke")} pending={revoke.isPending} onConfirm={() => { if (selected) revoke.mutate(selected.id) }} />
    </Card>
  )
}
