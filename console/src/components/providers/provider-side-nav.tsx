import { useMemo } from "react";
import { Link } from "@tanstack/react-router";
import { useQuery } from "@tanstack/react-query";
import { ArrowLeft } from "lucide-react";
import { useTranslation } from "react-i18next";
import { providersQuery } from "@/api/providers";
import { credentialModelStatusesQuery, credentialStatusesQuery } from "@/api/usage";
import { providerHealthLevels, ProviderSummary } from "@/components/providers/provider-summary";
import { useProviderToggle } from "@/components/providers/use-provider-toggle";
import { Switch } from "@/components/ui/switch";
import { cn } from "@/lib/utils";

/** 凭证最优先:数组顺序即导航顺序,credentials 同时是详情默认落点。 */
const PROVIDER_SECTIONS = [
  { to: "/providers/$providerId/credentials", label: "providers:tabs.credentials" },
  { to: "/providers/$providerId/settings", label: "providers:tabs.settings" },
  { to: "/providers/$providerId/models", label: "providers:tabs.models" },
  { to: "/providers/$providerId/routing-rules", label: "rules:tabs.routingRules" },
  { to: "/providers/$providerId/rule-sets", label: "rules:tabs.ruleSets" },
] as const;

function SectionLinks({ providerId, pill }: { providerId: string; pill?: boolean }) {
  const { t } = useTranslation(["providers", "rules"]);
  return (
    <>
      {PROVIDER_SECTIONS.map((s) => (
        <Link
          key={s.to}
          to={s.to}
          params={{ providerId }}
          activeProps={{ "data-active": "true" as const }}
          className={cn(
            "rounded-md px-3 py-1.5 text-sm text-muted-foreground",
            pill
              ? "shrink-0 whitespace-nowrap hover:text-foreground data-[active=true]:bg-background data-[active=true]:text-foreground data-[active=true]:shadow-sm"
              : "hover:bg-accent hover:text-accent-foreground data-[active=true]:bg-accent data-[active=true]:font-medium data-[active=true]:text-accent-foreground",
          )}
        >
          {t(s.label)}
        </Link>
      ))}
    </>
  );
}

/** 桌面端(md+)二级侧边栏:全部供应商可滚动列表,当前项展开分区(手风琴,展开态由路由推导)。 */
export function ProviderSideNav({ currentId }: { currentId: number }) {
  const { t } = useTranslation("providers");
  const { data: providers } = useQuery(providersQuery);
  const { data: statuses = [] } = useQuery(credentialStatusesQuery);
  const { data: modelStatuses = [] } = useQuery(credentialModelStatusesQuery);
  const health = useMemo(
    () => providerHealthLevels(statuses, modelStatuses),
    [statuses, modelStatuses],
  );
  const toggle = useProviderToggle();
  return (
    <aside className="sticky top-14 hidden h-[calc(100svh-3.5rem)] w-56 shrink-0 flex-col border-r md:flex">
      <Link
        to="/providers"
        className="flex items-center gap-2 px-4 py-3 text-sm font-medium text-muted-foreground hover:text-foreground"
      >
        <ArrowLeft className="size-4" aria-hidden />
        {t("sideNav.all")}
      </Link>
      <nav className="grid flex-1 content-start gap-0.5 overflow-y-auto px-2 pb-4">
        {(providers ?? []).map((p) =>
          p.id === currentId ? (
            <div key={p.id} className="grid gap-0.5">
              <div className="grid gap-0.5 rounded-md px-3 py-1.5">
                <div className="flex min-w-0 items-center justify-between gap-2">
                  <span className="truncate text-sm font-medium">{p.label ?? p.name}</span>
                  <Switch
                    size="sm"
                    checked={p.enabled}
                    disabled={toggle.isPending}
                    aria-label={t("fields.enabled")}
                    onCheckedChange={(enabled) => toggle.mutate({ provider: p, enabled })}
                  />
                </div>
                <ProviderSummary
                  channel={p.channel}
                  credentialCount={p.credential_count}
                  level={health.get(p.id) ?? "healthy"}
                />
              </div>
              <div className="grid gap-0.5 pl-3">
                <SectionLinks providerId={String(p.id)} />
              </div>
            </div>
          ) : (
            <div
              key={p.id}
              className={cn(
                "flex items-center gap-2 rounded-md pr-2 text-muted-foreground hover:bg-accent hover:text-accent-foreground",
                !p.enabled && "opacity-50",
              )}
            >
              <Link
                to="/providers/$providerId/credentials"
                params={{ providerId: String(p.id) }}
                className="grid min-w-0 flex-1 gap-0.5 px-3 py-1.5"
              >
                <span className="truncate text-sm">{p.label ?? p.name}</span>
                <ProviderSummary
                  channel={p.channel}
                  credentialCount={p.credential_count}
                  level={health.get(p.id) ?? "healthy"}
                />
              </Link>
              <Switch
                size="sm"
                checked={p.enabled}
                disabled={toggle.isPending}
                aria-label={t("fields.enabled")}
                onCheckedChange={(enabled) => toggle.mutate({ provider: p, enabled })}
              />
            </div>
          ),
        )}
      </nav>
    </aside>
  );
}

/** 移动端(<md)横向分区导航,视觉沿用 TabsList 风格。 */
export function ProviderSectionBar({ providerId }: { providerId: string }) {
  return (
    <nav className="flex gap-1 overflow-x-auto rounded-lg bg-muted p-1 md:hidden">
      <SectionLinks providerId={providerId} pill />
    </nav>
  );
}
