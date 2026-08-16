import { useQuery } from "@tanstack/react-query";
import { X } from "lucide-react";
import { useTranslation } from "react-i18next";
import { usersQuery } from "@/api/identity";
import type { AuditFilter } from "@/api/usage";
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

  return (
    <div className="flex flex-wrap items-center gap-2">
      <TimeRangePicker
        value={{ from: value.at_from, to: value.at_to }}
        onChange={(range) =>
          onChange({ ...value, at_from: range.from, at_to: range.to })
        }
      />

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
