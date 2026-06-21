import { useState } from "react";
import { useMutation, useQueryClient, useSuspenseQuery } from "@tanstack/react-query";
import { createFileRoute, redirect, useNavigate } from "@tanstack/react-router";
import { Trash2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import { deleteProvider, providerQuery } from "@/api/providers";
import { ApiError } from "@/api/http";
import { ConfirmDangerous } from "@/components/confirm-dangerous";
import { toast } from "sonner";
import { ProviderForm } from "@/components/providers/provider-form";
import { CredentialsTab } from "@/components/providers/credentials-tab";
import { ModelsTab } from "@/components/providers/models-tab";
import { RoutingRulesTab } from "@/components/providers/routing-rules-tab";
import { ProviderRulesTab } from "@/components/providers/provider-rules-tab";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";

export const Route = createFileRoute("/_app/providers/$providerId")({
  loader: ({ context, params }) => {
    const id = Number(params.providerId);
    if (Number.isNaN(id)) throw redirect({ to: "/providers" });
    return context.queryClient.ensureQueryData(providerQuery(id));
  },
  component: ProviderDetailPage,
});

function ProviderDetailPage() {
  const { providerId } = Route.useParams();
  const id = Number(providerId);
  const { t } = useTranslation("providers");
  const { t: tRules } = useTranslation("rules");
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const { data: provider } = useSuspenseQuery(providerQuery(id));
  const [deleteOpen, setDeleteOpen] = useState(false);

  const removal = useMutation({
    mutationFn: () => deleteProvider(id),
    onSuccess: () => {
      setDeleteOpen(false); // close before navigation unmounts → no double-click window
      void queryClient.invalidateQueries({ queryKey: ["providers"] });
      void navigate({ to: "/providers" });
    },
    onError: (error) => {
      toast.error(error instanceof ApiError ? error.message : String(error));
    },
  });

  return (
    <div className="grid gap-4 p-4 md:p-6">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div className="flex items-center gap-3">
          <h1 className="text-xl font-semibold">{provider.label ?? provider.name}</h1>
          <Badge variant="outline" className="font-mono">{provider.channel}</Badge>
          {!provider.enabled && <Badge variant="outline">off</Badge>}
        </div>
        <Button variant="ghost" size="sm" className="text-destructive" onClick={() => setDeleteOpen(true)}>
          <Trash2 className="size-4" />
          <span className="hidden sm:inline">{t("delete.provider")}</span>
        </Button>
      </div>

      <Tabs defaultValue="settings">
        <TabsList>
          <TabsTrigger value="settings">{t("tabs.settings")}</TabsTrigger>
          <TabsTrigger value="credentials">{t("tabs.credentials")}</TabsTrigger>
          <TabsTrigger value="models">{t("tabs.models")}</TabsTrigger>
          <TabsTrigger value="routing-rules">{tRules("tabs.routingRules")}</TabsTrigger>
          <TabsTrigger value="rule-sets">{tRules("tabs.ruleSets")}</TabsTrigger>
        </TabsList>
        <TabsContent value="settings" className="max-w-2xl pt-2">
          <ProviderForm provider={provider} onSaved={() => void 0} />
        </TabsContent>
        <TabsContent value="credentials" className="pt-2">
          <CredentialsTab provider={provider} />
        </TabsContent>
        <TabsContent value="models" className="pt-2">
          <ModelsTab provider={provider} />
        </TabsContent>
        <TabsContent value="routing-rules" className="pt-2">
          <RoutingRulesTab providerId={id} />
        </TabsContent>
        <TabsContent value="rule-sets" className="pt-2">
          <ProviderRulesTab providerId={id} />
        </TabsContent>
      </Tabs>

      <ConfirmDangerous
        open={deleteOpen}
        onOpenChange={setDeleteOpen}
        title={t("delete.provider")}
        description={t("delete.providerConfirm", { name: provider.label ?? provider.name })}
        confirmLabel={t("delete.provider")}
        onConfirm={() => removal.mutate()}
        pending={removal.isPending}
      />
    </div>
  );
}
