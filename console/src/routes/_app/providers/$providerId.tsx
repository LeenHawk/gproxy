import { useState } from "react";
import { useMutation, useQueryClient, useSuspenseQuery } from "@tanstack/react-query";
import { Link, Outlet, createFileRoute, redirect, useNavigate } from "@tanstack/react-router";
import { Trash2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import { deleteProvider, providerQuery } from "@/api/providers";
import { ApiError } from "@/api/http";
import { ConfirmDangerous } from "@/components/confirm-dangerous";
import { toast } from "sonner";
import { deleteProviderDefaultRuleSet } from "@/lib/provider-rule-set";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { useChannelMeta } from "@/hooks/use-channel-catalog";
import { cn } from "@/lib/utils";

const PROVIDER_SECTIONS = [
  { to: "/providers/$providerId/credentials", label: "providers:tabs.credentials" },
  { to: "/providers/$providerId/models", label: "providers:tabs.models" },
  { to: "/providers/$providerId/routing-rules", label: "rules:tabs.routingRules" },
  { to: "/providers/$providerId/rule-sets", label: "rules:tabs.ruleSets" },
  { to: "/providers/$providerId/settings", label: "providers:tabs.settings" },
] as const;

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
  const { t } = useTranslation(["providers", "rules"]);
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const { data: provider } = useSuspenseQuery(providerQuery(id));
  const { meta } = useChannelMeta(provider.channel);
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
    <div className="grid gap-4 p-4 md:p-6">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div className="flex flex-wrap items-center gap-3">
          <h1 className="text-xl font-semibold">{provider.label ?? provider.name}</h1>
          <Badge variant="outline" className="gap-1.5">
            {meta?.displayName ?? provider.channel}
            {meta && meta.displayName !== provider.channel && (
              <span className="font-mono text-[0.65rem] text-muted-foreground">{provider.channel}</span>
            )}
          </Badge>
          {!provider.enabled && <Badge variant="outline">off</Badge>}
        </div>
        <Button variant="ghost" size="sm" className="text-destructive" onClick={() => setDeleteOpen(true)}>
          <Trash2 className="size-4" aria-hidden />
          <span className="hidden sm:inline">{t("delete.provider")}</span>
        </Button>
      </div>

      <nav className="flex gap-1 overflow-x-auto rounded-lg bg-muted p-1">
        {PROVIDER_SECTIONS.map((section) => (
          <Link
            key={section.to}
            to={section.to}
            params={{ providerId }}
            activeProps={{ "data-active": "true" as const }}
            className={cn(
              "shrink-0 whitespace-nowrap rounded-md px-3 py-1.5 text-sm text-muted-foreground",
              "hover:text-foreground data-[active=true]:bg-background data-[active=true]:text-foreground data-[active=true]:shadow-sm",
            )}
          >
            {t(section.label)}
          </Link>
        ))}
      </nav>
      <Outlet />

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
