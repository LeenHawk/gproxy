import { useState } from "react";
import { useMutation, useQuery, useQueryClient, useSuspenseQuery } from "@tanstack/react-query";
import { createFileRoute, redirect, useNavigate } from "@tanstack/react-router";
import { Pencil, Trash2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { deleteUser, orgsQuery, teamsQuery, userQuery } from "@/api/identity";
import { ApiError } from "@/api/http";
import { EntityDialog } from "@/components/entity-dialog";
import { ScopeAccessEditor } from "@/components/identity/scope-access-editor";
import { UserForm } from "@/components/identity/user-form";
import { UserKeysTab } from "@/components/identity/user-keys-tab";
import { ConfirmDangerous } from "@/components/confirm-dangerous";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";

export const Route = createFileRoute("/_app/users/$userId")({
  loader: ({ context, params }) => {
    const id = Number(params.userId);
    if (Number.isNaN(id)) throw redirect({ to: "/users" });
    return context.queryClient.ensureQueryData(userQuery(id));
  },
  component: UserDetailPage,
});

function UserDetailPage() {
  const { userId } = Route.useParams();
  const id = Number(userId);
  const { t } = useTranslation("identity");
  const { t: tc } = useTranslation("common");
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const { data: user } = useSuspenseQuery(userQuery(id));
  const [deleteOpen, setDeleteOpen] = useState(false);
  const [editOpen, setEditOpen] = useState(false);

  const { data: orgs } = useQuery(orgsQuery);
  const { data: teams } = useQuery(teamsQuery(user.org_id));

  const orgName = orgs?.find((o) => o.id === user.org_id)?.name ?? `#${user.org_id}`;
  const teamName = user.team_id !== null
    ? (teams?.find((tm) => tm.id === user.team_id)?.name ?? `#${user.team_id}`)
    : null;

  const removal = useMutation({
    mutationFn: () => deleteUser(id),
    onSuccess: () => {
      toast.success(tc("actions.deleted"));
      setDeleteOpen(false);
      void queryClient.invalidateQueries({ queryKey: ["users"] });
      void navigate({ to: "/users" });
    },
    onError: (error) => {
      toast.error(error instanceof ApiError ? error.message : String(error));
    },
  });

  return (
    <div className="grid gap-4 p-4 md:p-6">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div className="flex flex-wrap items-center gap-3">
          <h1 className="text-xl font-semibold">{user.name}</h1>
          {user.is_admin && (
            <Badge variant="secondary">{t("users.isAdmin")}</Badge>
          )}
          <Badge variant={user.enabled ? "secondary" : "outline"}>{user.enabled ? "on" : "off"}</Badge>
          <span className="text-sm text-muted-foreground">{orgName}</span>
          {teamName && (
            <span className="text-sm text-muted-foreground">/ {teamName}</span>
          )}
        </div>
        <div className="flex items-center gap-1">
          <Button variant="outline" size="sm" onClick={() => setEditOpen(true)}>
            <Pencil className="size-4" aria-hidden />
            <span className="hidden sm:inline">{t("users.edit")}</span>
          </Button>
          <Button variant="ghost" size="sm" className="text-destructive" onClick={() => setDeleteOpen(true)}>
            <Trash2 className="size-4" aria-hidden />
            <span className="hidden sm:inline">{t("users.delete")}</span>
          </Button>
        </div>
      </div>

      <Tabs defaultValue="keys">
        <TabsList>
          <TabsTrigger value="keys">{t("users.tabs.keys")}</TabsTrigger>
          <TabsTrigger value="access">{t("users.tabs.access")}</TabsTrigger>
        </TabsList>
        <TabsContent value="keys" className="pt-2">
          <UserKeysTab user={user} />
        </TabsContent>
        <TabsContent value="access" className="max-w-2xl pt-4">
          <ScopeAccessEditor scope="user" scopeId={user.id} />
        </TabsContent>
      </Tabs>

      <EntityDialog open={editOpen} onOpenChange={setEditOpen} title={t("users.edit")}>
        <UserForm user={user} onSaved={() => setEditOpen(false)} />
      </EntityDialog>

      <ConfirmDangerous
        open={deleteOpen}
        onOpenChange={setDeleteOpen}
        title={t("users.delete")}
        description={t("users.deleteConfirm", { name: user.name })}
        confirmLabel={t("users.delete")}
        onConfirm={() => removal.mutate()}
        pending={removal.isPending}
      />
    </div>
  );
}
