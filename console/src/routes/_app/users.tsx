import { useMemo } from "react";
import { useMutation, useQueries, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link, Outlet, createFileRoute, useRouterState } from "@tanstack/react-router";
import { ArrowLeft, Plus, Users } from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { ApiError } from "@/api/http";
import { orgsQuery, teamsQuery, upsertUser, usersQuery, type UserView } from "@/api/identity";
import { WorkspaceBatchBar } from "@/components/workspace/workspace-batch-bar";
import { WorkspaceLayout } from "@/components/workspace/workspace-layout";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import { useBatch } from "@/hooks/use-batch";

export const Route = createFileRoute("/_app/users")({
  loader: ({ context }) => {
    void context.queryClient.ensureQueryData(orgsQuery);
    return context.queryClient.ensureQueryData(usersQuery);
  },
  component: UsersWorkspace,
});

function UsersWorkspace() {
  const { t } = useTranslation("identity");
  const { t: tc } = useTranslation("common");
  const queryClient = useQueryClient();
  const pathname = useRouterState({ select: (state) => state.location.pathname });
  const selectedId = /(?:^|\/)users\/([^/]+)/.exec(pathname)?.[1] ?? null;
  const { data: users } = useQuery(usersQuery);
  const { data: orgs } = useQuery(orgsQuery);
  const rows = users ?? [];
  const batch = useBatch("users", ["users"]);
  const orgIds = useMemo(() => [...new Set(rows.map((user) => user.org_id))], [rows]);
  const teamQueries = useQueries({ queries: orgIds.map((id) => teamsQuery(id)) });
  const orgNames = new Map((orgs ?? []).map((org) => [org.id, org.name]));
  const teamNames = new Map(teamQueries.flatMap((query) => query.data ?? []).map((team) => [team.id, team.name]));

  const toggle = useMutation({
    mutationFn: ({ user, enabled }: { user: UserView; enabled: boolean }) =>
      upsertUser({
        id: user.id,
        name: user.name,
        org_id: user.org_id,
        team_id: user.team_id,
        enabled,
        is_admin: user.is_admin,
      }),
    onMutate: async ({ user, enabled }) => {
      await queryClient.cancelQueries({ queryKey: ["users"] });
      const previous = queryClient.getQueryData<UserView[]>(usersQuery.queryKey);
      queryClient.setQueryData<UserView[]>(usersQuery.queryKey, (current) =>
        current?.map((item) => (item.id === user.id ? { ...item, enabled } : item)),
      );
      queryClient.setQueryData(["users", user.id], (current: UserView | undefined) =>
        current ? { ...current, enabled } : current,
      );
      return { previous };
    },
    onError: (error, _variables, context) => {
      if (context?.previous) queryClient.setQueryData(usersQuery.queryKey, context.previous);
      toast.error(error instanceof ApiError ? error.message : String(error));
    },
    onSettled: () => void queryClient.invalidateQueries({ queryKey: ["users"] }),
  });

  return (
    <WorkspaceLayout
      title={t("users.title")}
      items={rows}
      selectedId={selectedId}
      getId={(user) => user.id}
      getSearchText={(user) => `${user.name} ${orgNames.get(user.org_id) ?? ""} ${user.team_id ? teamNames.get(user.team_id) ?? "" : ""}`}
      renderTitle={(user) => user.name}
      renderSummary={(user) => (
        <>
          <span className="truncate">{orgNames.get(user.org_id) ?? `#${user.org_id}`} · {user.team_id ? teamNames.get(user.team_id) ?? `#${user.team_id}` : t("users.noTeam")}</span>
          {user.is_admin && <Badge variant="secondary" className="shrink-0 px-1 py-0 text-[10px]">{t("users.isAdmin")}</Badge>}
        </>
      )}
      renderLink={(user, content, className) => (
        <Link to="/users/$userId" params={{ userId: String(user.id) }} search={{ tab: "profile" }} className={className}>{content}</Link>
      )}
      renderAction={(user) => (
        <Switch
          size="sm"
          checked={user.enabled}
          disabled={toggle.isPending}
          aria-label={user.enabled ? t("users.disable") : t("users.enable")}
          onCheckedChange={(enabled) => toggle.mutate({ user, enabled })}
        />
      )}
      searchPlaceholder={t("users.search")}
      emptyLabel={t("users.empty")}
      createAction={(
        <Button asChild size="icon-sm" aria-label={t("users.new")}>
          <Link to="/users/new"><Plus aria-hidden /></Link>
        </Button>
      )}
      batch={{
        active: batch.mode,
        selectedIds: batch.selected as ReadonlySet<number>,
        entryLabel: tc("batch.select"),
        cancelLabel: tc("batch.cancel"),
        selectAllLabel: t("users.selectAll"),
        onEnter: () => batch.setMode(true),
        onExit: batch.exit,
        onToggle: batch.toggle,
        onToggleAll: batch.toggleAllFor,
        footer: (
          <WorkspaceBatchBar
            count={batch.selected.size}
            pending={batch.pending}
            onEnable={batch.runEnable}
            onDisable={batch.runDisable}
            onDelete={batch.runDelete}
          />
        ),
      }}
      mobileBack={(
        <Link to="/users" className="inline-flex items-center gap-2 text-sm text-muted-foreground">
          <ArrowLeft className="size-4" aria-hidden />{t("users.title")}
        </Link>
      )}
      emptyState={(
        <div className="hidden h-full min-h-[28rem] place-content-center text-center text-muted-foreground md:grid">
          <Users className="mx-auto mb-3 size-8" aria-hidden />
          <p className="text-sm">{t("users.selectHint")}</p>
        </div>
      )}
    >
      <Outlet />
    </WorkspaceLayout>
  );
}
