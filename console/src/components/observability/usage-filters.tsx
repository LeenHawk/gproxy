import { useQuery } from "@tanstack/react-query";
import { X } from "lucide-react";
import { useTranslation } from "react-i18next";
import { credentialsQuery } from "@/api/credentials";
import { providersQuery } from "@/api/providers";
import { usersQuery } from "@/api/identity";
import { routesQuery } from "@/api/routes";
import type { UsageFilter } from "@/api/usage";
import { TimeRangePicker } from "@/components/time-range-picker";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";

interface UsageFiltersProps {
  value: Omit<UsageFilter, "before_id" | "limit">;
  onChange: (f: Omit<UsageFilter, "before_id" | "limit">) => void;
  showModel?: boolean;
  showCredential?: boolean;
  routeListId?: string;
}

export function UsageFilters({
  value,
  onChange,
  showModel = true,
  showCredential = false,
  routeListId = "route-datalist",
}: UsageFiltersProps) {
  const { t } = useTranslation("observability");
  const { data: providers } = useQuery(providersQuery);
  const { data: credentials } = useQuery({
    ...credentialsQuery(value.provider_id ?? 0),
    enabled: showCredential && value.provider_id != null,
  });
  const { data: users } = useQuery(usersQuery);
  const { data: routes } = useQuery(routesQuery);

  function setField<K extends keyof typeof value>(k: K, v: (typeof value)[K]) {
    onChange({ ...value, [k]: v });
  }

  function reset() {
    onChange({});
  }

  return (
    <div className="flex flex-wrap items-center gap-2">
      <TimeRangePicker
        value={{ from: value.at_from, to: value.at_to }}
        onChange={(r) => onChange({ ...value, at_from: r.from, at_to: r.to })}
      />

      {/* Provider */}
      <Select
        value={value.provider_id != null ? String(value.provider_id) : ""}
        onValueChange={(v) => onChange({
          ...value,
          provider_id: v && v !== "__all__" ? Number(v) : undefined,
          credential_id: undefined,
        })}
      >
        <SelectTrigger size="sm" className="w-36">
          <SelectValue placeholder={t("usage.filters.provider")} />
        </SelectTrigger>
        <SelectContent>
          <SelectItem value="__all__">{t("usage.filters.provider")}</SelectItem>
          {(providers ?? []).map((p) => (
            <SelectItem key={p.id} value={String(p.id)}>
              {p.label ?? p.name}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>

      {showCredential && (
        <Select
          value={value.credential_id != null ? String(value.credential_id) : ""}
          onValueChange={(v) =>
            setField("credential_id", v && v !== "__all__" ? Number(v) : undefined)
          }
          disabled={value.provider_id == null}
        >
          <SelectTrigger size="sm" className="w-36">
            <SelectValue placeholder={t("usage.filters.credential")} />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="__all__">{t("usage.filters.credential")}</SelectItem>
            {(credentials ?? []).map((credential) => (
              <SelectItem key={credential.id} value={String(credential.id)}>
                {credential.label ?? `#${credential.id}`}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      )}

      {/* User */}
      <Select
        value={value.user_id != null ? String(value.user_id) : ""}
        onValueChange={(v) =>
          setField("user_id", v && v !== "__all__" ? Number(v) : undefined)
        }
      >
        <SelectTrigger size="sm" className="w-36">
          <SelectValue placeholder={t("usage.filters.user")} />
        </SelectTrigger>
        <SelectContent>
          <SelectItem value="__all__">{t("usage.filters.user")}</SelectItem>
          {(users ?? []).map((u) => (
            <SelectItem key={u.id} value={String(u.id)}>
              {u.name}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>

      {/* Route name (input + datalist) */}
      <div className="relative">
        <Input
          size={16}
          placeholder={t("usage.filters.route")}
          value={value.route_name ?? ""}
          onChange={(e) => setField("route_name", e.target.value || undefined)}
          list={routeListId}
          className="h-8 text-sm"
        />
        <datalist id={routeListId}>
          {(routes ?? []).map((r) => (
            <option key={r.id} value={r.name} />
          ))}
        </datalist>
      </div>

      {showModel && (
        <Input
          size={14}
          placeholder={t("usage.filters.model")}
          value={value.model ?? ""}
          onChange={(e) => setField("model", e.target.value || undefined)}
          className="h-8 text-sm"
        />
      )}

      {/* Clear */}
      <Button
        variant="ghost"
        size="sm"
        onClick={reset}
        className="gap-1 text-muted-foreground"
      >
        <X className="size-3" aria-hidden />
        {t("usage.filters.reset")}
      </Button>
    </div>
  );
}
