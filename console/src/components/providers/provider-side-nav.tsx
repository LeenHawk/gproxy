import { Link } from "@tanstack/react-router";
import { useQuery } from "@tanstack/react-query";
import { ArrowLeft } from "lucide-react";
import { useTranslation } from "react-i18next";
import { providersQuery } from "@/api/providers";
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
              <div className="truncate rounded-md px-3 py-1.5 text-sm font-medium">{p.label ?? p.name}</div>
              <div className="grid gap-0.5 pl-3">
                <SectionLinks providerId={String(p.id)} />
              </div>
            </div>
          ) : (
            <Link
              key={p.id}
              to="/providers/$providerId/credentials"
              params={{ providerId: String(p.id) }}
              className={cn(
                "truncate rounded-md px-3 py-1.5 text-sm text-muted-foreground hover:bg-accent hover:text-accent-foreground",
                !p.enabled && "opacity-50",
              )}
            >
              {p.label ?? p.name}
            </Link>
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
