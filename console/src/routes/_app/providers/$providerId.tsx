import { useState } from "react";
import { useMutation, useQueryClient, useSuspenseQuery } from "@tanstack/react-query";
import { Outlet, createFileRoute, redirect, useNavigate } from "@tanstack/react-router";
import { Trash2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import { deleteProvider, providerQuery } from "@/api/providers";
import { ApiError } from "@/api/http";
import { ConfirmDangerous } from "@/components/confirm-dangerous";
import { toast } from "sonner";
import { deleteProviderDefaultRuleSet } from "@/lib/provider-rule-set";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";

export const Route = createFileRoute("/_app/providers/$providerId")({
  loader: ({ context, params }) => {
    const id = Number(params.providerId);
    if (Number.isNaN(id)) throw redirect({ to: "/providers" });
    return context.queryClient.ensureQueryData(providerQuery(id));
  },
  component: ProviderDetailLayout,
});

function ProviderDetailLayout() {
  const { providerId } = Route.useParams();
  const id = Number(providerId);
  const { t } = useTranslation("providers");
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const { data: provider } = useSuspenseQuery(providerQuery(id));
  const [deleteOpen, setDeleteOpen] = useState(false);

  const removal = useMutation({
    mutationFn: async () => {
      try {
        await deleteProviderDefaultRuleSet(id);
      } catch {
        // best-effort cleanup; never block provider deletion on it
      }
      await deleteProvider(id);
    },
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
    <div className="flex min-h-full">
      {/* Task 2: <ProviderSideNav currentId={id} /> */}
      <div className="min-w-0 flex-1">
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
          {/* Task 2: <ProviderSectionBar providerId={providerId} /> */}
          <Outlet />
        </div>
      </div>

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
