import { X } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { MyUsageFilter } from "@/api/portal";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";

const TIME_PRESETS = [
  { key: "1h", secs: 3_600 },
  { key: "24h", secs: 86_400 },
  { key: "7d", secs: 7 * 86_400 },
] as const;
type PresetKey = (typeof TIME_PRESETS)[number]["key"] | "all";

function presetToAtFrom(key: PresetKey): number | undefined {
  if (key === "all") return undefined;
  const secs = TIME_PRESETS.find((p) => p.key === key)?.secs;
  return secs !== undefined ? Math.floor(Date.now() / 1000) - secs : undefined;
}

interface MyUsageFiltersProps {
  value: MyUsageFilter;
  onChange: (f: MyUsageFilter) => void;
}

export function MyUsageFilters({ value, onChange }: MyUsageFiltersProps) {
  const { t } = useTranslation("portal");

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

  function setField<K extends keyof MyUsageFilter>(k: K, v: MyUsageFilter[K]) {
    onChange({ ...value, [k]: v });
  }

  const currentPreset = detectPreset();

  return (
    <div className="flex flex-wrap items-center gap-2">
      {/* Time range presets — no Select/empty-value: use button group */}
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
            {t(`usage.preset.${key}`)}
          </button>
        ))}
      </div>

      {/* Route name */}
      <Input
        size={16}
        placeholder={t("usage.route")}
        value={value.route_name ?? ""}
        onChange={(e) => setField("route_name", e.target.value || undefined)}
        className="h-8 text-sm"
      />

      {/* Model */}
      <Input
        size={14}
        placeholder={t("usage.model")}
        value={value.model ?? ""}
        onChange={(e) => setField("model", e.target.value || undefined)}
        className="h-8 text-sm"
      />

      {/* Clear */}
      <Button
        variant="ghost"
        size="sm"
        onClick={() => onChange({})}
        className="gap-1 text-muted-foreground"
      >
        <X className="size-3" aria-hidden />
        {t("usage.clearFilters")}
      </Button>
    </div>
  );
}
