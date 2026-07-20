import { useQuery } from "@tanstack/react-query";
import { X } from "lucide-react";
import { useTranslation } from "react-i18next";
import { usersQuery } from "@/api/identity";
import type { AuditFilter } from "@/api/usage";
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

function presetToAtFrom(key: PresetKey): number | undefined {
  if (key === "all") return undefined;
  const secs = TIME_PRESETS.find((preset) => preset.key === key)?.secs;
  return secs == null ? undefined : Math.floor(Date.now() / 1000) - secs;
}

interface AuditFiltersProps {
  value: AuditFilter;
  onChange: (filter: AuditFilter) => void;
}

export function AuditFilters({ value, onChange }: AuditFiltersProps) {
  const { t } = useTranslation("observability");
  const { data: users } = useQuery(usersQuery);

  function setField<K extends keyof AuditFilter>(
    key: K,
    next: AuditFilter[K],
  ) {
    onChange({ ...value, [key]: next });
  }

  function detectPreset(): PresetKey {
    if (value.at_from == null) return "all";
    const elapsed = Math.floor(Date.now() / 1000) - value.at_from;
    return (
      TIME_PRESETS.find((preset) => Math.abs(elapsed - preset.secs) < 60)?.key ??
      "all"
    );
  }

  function changePreset(key: PresetKey) {
    onChange({
      ...value,
      at_from: presetToAtFrom(key),
      at_to: undefined,
    });
  }

  const currentPreset = detectPreset();

  return (
    <div className="flex flex-wrap items-center gap-2">
      <div className="flex rounded-md border">
        {(["1h", "24h", "7d", "all"] as PresetKey[]).map((key) => (
          <button
            key={key}
            type="button"
            onClick={() => changePreset(key)}
            className={
              key === currentPreset
                ? "px-3 py-1.5 text-xs font-medium bg-primary text-primary-foreground first:rounded-l-md last:rounded-r-md"
                : "px-3 py-1.5 text-xs text-muted-foreground hover:bg-accent hover:text-accent-foreground first:rounded-l-md last:rounded-r-md"
            }
          >
            {t(`usage.presets.${key}`)}
          </button>
        ))}
      </div>

      <Select
        value={value.actor_id != null ? String(value.actor_id) : ""}
        onValueChange={(next) =>
          setField(
            "actor_id",
            next && next !== "__all__" ? Number(next) : undefined,
          )
        }
      >
        <SelectTrigger size="sm" className="w-36">
          <SelectValue placeholder={t("audit.filters.actor")} />
        </SelectTrigger>
        <SelectContent>
          <SelectItem value="__all__">{t("audit.filters.actor")}</SelectItem>
          {(users ?? []).map((user) => (
            <SelectItem key={user.id} value={String(user.id)}>
              {user.name}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>

      <Input
        size={14}
        placeholder={t("audit.filters.action")}
        value={value.action ?? ""}
        onChange={(event) =>
          setField("action", event.target.value || undefined)
        }
        className="h-8 text-sm"
      />
      <Input
        size={14}
        placeholder={t("audit.filters.target")}
        value={value.target ?? ""}
        onChange={(event) =>
          setField("target", event.target.value || undefined)
        }
        className="h-8 text-sm"
      />
      <Input
        type="number"
        size={8}
        placeholder={t("audit.filters.status")}
        value={value.status ?? ""}
        onChange={(event) =>
          setField(
            "status",
            event.target.value ? Number(event.target.value) : undefined,
          )
        }
        className="h-8 text-sm"
      />
      <Input
        size={14}
        placeholder={t("audit.filters.sourceIp")}
        value={value.source_ip ?? ""}
        onChange={(event) =>
          setField("source_ip", event.target.value || undefined)
        }
        className="h-8 text-sm"
      />

      <Button
        variant="ghost"
        size="sm"
        onClick={() => onChange({})}
        className="gap-1 text-muted-foreground"
      >
        <X className="size-3" aria-hidden />
        {t("audit.filters.reset")}
      </Button>
    </div>
  );
}
