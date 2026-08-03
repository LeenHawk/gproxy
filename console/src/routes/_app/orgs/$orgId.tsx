import { useState } from "react";
import { useMutation, useQueryClient, useSuspenseQuery } from "@tanstack/react-query";
import { createFileRoute, redirect, useNavigate } from "@tanstack/react-router";
import { Trash2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { deleteOrg, orgQuery } from "@/api/identity";
import { ApiError } from "@/api/http";
import { OrgForm } from "@/components/identity/org-form";
import { ScopeAccessEditor } from "@/components/identity/scope-access-editor";
import { TeamsTab } from "@/components/identity/teams-tab";
import { ConfirmDangerous } from "@/components/confirm-dangerous";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";

export const Route = createFileRoute("/_app/orgs/$orgId")({
  validateSearch: (search: Record<string, unknown>): { tab: OrgTab } => ({
    tab: isOrgTab(search.tab) ? search.tab : "profile",
  }),
  loader: ({ context, params }) => {
    const id = Number(params.orgId);
    if (Number.isNaN(id)) throw redirect({ to: "/orgs" });
    return context.queryClient.ensureQueryData(orgQuery(id));
  },
  component: OrgDetailPage,
});

function OrgDetailPage() {
  const { orgId } = Route.useParams();
  const id = Number(orgId);
  const { t } = useTranslation("identity");
  const { t: tc } = useTranslation("common");
  const { tab } = Route.useSearch();
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const { data: org } = useSuspenseQuery(orgQuery(id));
  const [deleteOpen, setDeleteOpen] = useState(false);

  const removal = useMutation({
    mutationFn: () => deleteOrg(id),
    onSuccess: () => {
      toast.success(tc("actions.deleted"));
      setDeleteOpen(false);
      void queryClient.invalidateQueries({ queryKey: ["orgs"] });
      void navigate({ to: "/orgs" });
    },
    onError: (error) => {
      toast.error(error instanceof ApiError ? error.message : String(error));
    },
  });

  return (
    <div className="grid gap-4 p-4 md:p-6">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div className="flex items-center gap-3">
          <h1 className="text-xl font-semibold">{org.name}</h1>
          <Badge variant={org.enabled ? "secondary" : "outline"}>{org.enabled ? "on" : "off"}</Badge>
        </div>
        <Button variant="ghost" size="sm" className="text-destructive" onClick={() => setDeleteOpen(true)}>
          <Trash2 className="size-4" aria-hidden />
          <span className="hidden sm:inline">{t("orgs.delete")}</span>
        </Button>
      </div>

      <Tabs
        value={tab}
        onValueChange={(value) => {
          void navigate({
            to: "/orgs/$orgId",
            params: { orgId },
            search: { tab: value as OrgTab },
            replace: true,
          });
        }}
      >
        <TabsList>
          <TabsTrigger value="profile">{t("orgs.tabs.profile")}</TabsTrigger>
          <TabsTrigger value="teams">{t("orgs.tabs.teams")}</TabsTrigger>
          <TabsTrigger value="access">{t("orgs.tabs.access")}</TabsTrigger>
        </TabsList>
        <TabsContent value="profile" className="max-w-xl pt-4">
          <div className="rounded-lg border p-4">
            <OrgForm key={`${org.id}-${org.updated_at}`} org={org} onSaved={() => undefined} />
          </div>
        </TabsContent>
        <TabsContent value="teams" className="pt-2">
          <TeamsTab org={org} />
        </TabsContent>
        <TabsContent value="access" className="max-w-2xl pt-4">
          <ScopeAccessEditor scope="org" scopeId={org.id} />
        </TabsContent>
      </Tabs>

      <ConfirmDangerous
        open={deleteOpen}
        onOpenChange={setDeleteOpen}
        title={t("orgs.delete")}
        description={t("orgs.deleteConfirm", { name: org.name })}
        confirmLabel={t("orgs.delete")}
        onConfirm={() => removal.mutate()}
        pending={removal.isPending}
      />
    </div>
  );
}

type OrgTab = "profile" | "teams" | "access";

function isOrgTab(value: unknown): value is OrgTab {
  return value === "profile" || value === "teams" || value === "access";
}
