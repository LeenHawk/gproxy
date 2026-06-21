import { useQuery } from "@tanstack/react-query";
import { X } from "lucide-react";
import { useTranslation } from "react-i18next";
import { providersQuery } from "@/api/providers";
import { usersQuery } from "@/api/identity";
import { routesQuery } from "@/api/routes";
import type { UsageFilter } from "@/api/usage";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";

const TIME_PRESETS = [
  { key: "1h", secs: 3_600 },
  { key: "24h", secs: 86_400 },
  { key: "7d", secs: 7 * 86_400 },
] as const;
type PresetKey = (typeof TIME_PRESETS)[number]["key"] | "all";

// Derives at_from from a preset key (undefined = no filter)
function presetToAtFrom(key: PresetKey): number | undefined {
  if (key === "all") return undefined;
  const secs = TIME_PRESETS.find((p) => p.key === key)?.secs;
  return secs !== undefined ? Math.floor(Date.now() / 1000) - secs : undefined;
}

interface UsageFiltersProps {
  value: Omit<UsageFilter, "before_id" | "limit">;
  onChange: (f: Omit<UsageFilter, "before_id" | "limit">) => void;
}

export function UsageFilters({ value, onChange }: UsageFiltersProps) {
  const { t } = useTranslation("observability");
  const { data: providers } = useQuery(providersQuery);
  const { data: users } = useQuery(usersQuery);
  const { data: routes } = useQuery(routesQuery);

  function setField<K extends keyof typeof value>(k: K, v: (typeof value)[K]) {
    onChange({ ...value, [k]: v });
  }

  // Detect current time preset from at_from (approximate; "all" if unset)
  function detectPreset(): PresetKey {
    if (value.at_from == null) return "all";
    const now = Math.floor(Date.now() / 1000);
    const diff = now - value.at_from;
    for (const p of TIME_PRESETS) {
      if (Math.abs(diff - p.secs) < 60) return p.key;
    }
    return "all";
  }

  function onPresetChange(key: PresetKey) {
    onChange({ ...value, at_from: presetToAtFrom(key), at_to: undefined });
  }

  function reset() {
    onChange({});
  }

  const currentPreset = detectPreset();

  return (
    <div className="flex flex-wrap items-center gap-2">
      {/* Time range presets */}
      <div className="flex rounded-md border">
        {(["1h", "24h", "7d", "all"] as PresetKey[]).map((key) => (
          <button
            key={key}
            type="button"
            onClick={() => onPresetChange(key)}
            className={
              key === currentPreset
                ? "px-3 py-1.5 text-xs font-medium bg-primary text-primary-foreground first:rounded-l-md last:rounded-r-md"
                : "px-3 py-1.5 text-xs text-muted-foreground hover:bg-accent hover:text-accent-foreground first:rounded-l-md last:rounded-r-md"
            }
          >
            {key === "all" ? t("usage.presets.all", { defaultValue: "All" }) : t(`usage.presets.${key}`, { defaultValue: key })}
          </button>
        ))}
      </div>

      {/* Provider */}
      <Select
        value={value.provider_id != null ? String(value.provider_id) : ""}
        onValueChange={(v) =>
          setField("provider_id", v && v !== "__all__" ? Number(v) : undefined)
        }
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
          list="route-datalist"
          className="h-8 text-sm"
        />
        <datalist id="route-datalist">
          {(routes ?? []).map((r) => (
            <option key={r.id} value={r.name} />
          ))}
        </datalist>
      </div>

      {/* Model */}
      <Input
        size={14}
        placeholder={t("usage.filters.model")}
        value={value.model ?? ""}
        onChange={(e) => setField("model", e.target.value || undefined)}
        className="h-8 text-sm"
      />

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
