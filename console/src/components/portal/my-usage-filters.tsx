import { X } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { MyUsageFilter } from "@/api/portal";
import { TimeRangePicker } from "@/components/time-range-picker";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";

interface MyUsageFiltersProps {
  value: MyUsageFilter;
  onChange: (f: MyUsageFilter) => void;
}

export function MyUsageFilters({ value, onChange }: MyUsageFiltersProps) {
  const { t } = useTranslation("portal");

  function setField<K extends keyof MyUsageFilter>(k: K, v: MyUsageFilter[K]) {
    onChange({ ...value, [k]: v });
  }

  return (
    <div className="flex flex-wrap items-center gap-2">
      <TimeRangePicker
        value={{ from: value.at_from, to: value.at_to }}
        onChange={(r) => onChange({ ...value, at_from: r.from, at_to: r.to })}
      />

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
