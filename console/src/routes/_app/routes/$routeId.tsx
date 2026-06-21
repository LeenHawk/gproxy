import { useState } from "react";
import { useMutation, useQueryClient, useSuspenseQuery } from "@tanstack/react-query";
import { createFileRoute, redirect, useNavigate } from "@tanstack/react-router";
import { Trash2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { deleteRoute, routeQuery } from "@/api/routes";
import { ApiError } from "@/api/http";
import { ConfirmDangerous } from "@/components/confirm-dangerous";
import { RouteForm } from "@/components/routes/route-form";
import { MembersTab } from "@/components/routes/members-tab";
import { AliasesTab } from "@/components/routes/aliases-tab";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";

export const Route = createFileRoute("/_app/routes/$routeId")({
  loader: ({ context, params }) => {
    const id = Number(params.routeId);
    if (Number.isNaN(id)) throw redirect({ to: "/routes" });
    return context.queryClient.ensureQueryData(routeQuery(id));
  },
  component: RouteDetailPage,
});

function RouteDetailPage() {
  const { routeId } = Route.useParams();
  const id = Number(routeId);
  const { t } = useTranslation("routes");
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const { data: route } = useSuspenseQuery(routeQuery(id));
  const [deleteOpen, setDeleteOpen] = useState(false);

  const removal = useMutation({
    mutationFn: () => deleteRoute(id),
    onSuccess: () => {
      setDeleteOpen(false); // close before navigation unmounts → no double-click window
      void queryClient.invalidateQueries({ queryKey: ["routes"] });
      void navigate({ to: "/routes" });
    },
    onError: (error) => toast.error(error instanceof ApiError ? error.message : String(error)),
  });

  return (
    <div className="grid gap-4 p-4 md:p-6">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div className="flex items-center gap-3">
          <h1 className="text-xl font-semibold">{route.name}</h1>
          <Badge variant="outline">{t(`strategy.${route.strategy}`, { defaultValue: route.strategy })}</Badge>
          {!route.enabled && <Badge variant="outline">off</Badge>}
        </div>
        <Button variant="ghost" size="sm" className="text-destructive" onClick={() => setDeleteOpen(true)}>
          <Trash2 className="size-4" />
          <span className="hidden sm:inline">{t("delete.route")}</span>
        </Button>
      </div>

      <Tabs defaultValue="settings">
        <TabsList>
          <TabsTrigger value="settings">{t("tabs.settings")}</TabsTrigger>
          <TabsTrigger value="members">{t("tabs.members")}</TabsTrigger>
          <TabsTrigger value="aliases">{t("tabs.aliases")}</TabsTrigger>
        </TabsList>
        <TabsContent value="settings" className="max-w-2xl pt-2">
          <RouteForm route={route} onSaved={() => void 0} />
        </TabsContent>
        <TabsContent value="members" className="pt-2">
          <MembersTab route={route} />
        </TabsContent>
        <TabsContent value="aliases" className="pt-2">
          <AliasesTab route={route} />
        </TabsContent>
      </Tabs>

      <ConfirmDangerous
        open={deleteOpen}
        onOpenChange={setDeleteOpen}
        title={t("delete.route")}
        description={t("delete.routeConfirm", { name: route.name })}
        confirmLabel={t("delete.route")}
        onConfirm={() => removal.mutate()}
        pending={removal.isPending}
      />
    </div>
  );
}
