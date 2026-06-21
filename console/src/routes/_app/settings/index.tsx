import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { useTranslation } from "react-i18next";
import { instanceSettingsQuery, type InstanceSettings } from "@/api/settings";
import { SettingsForm } from "@/components/settings/settings-form";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Skeleton } from "@/components/ui/skeleton";

export const Route = createFileRoute("/_app/settings/")({
  loader: ({ context }) => {
    void context.queryClient.ensureQueryData(instanceSettingsQuery);
  },
  component: SettingsPage,
});

function SettingsPage() {
  const { t } = useTranslation("settings");
  const { data: list = [], isPending } = useQuery(instanceSettingsQuery);
  const [selectedId, setSelectedId] = useState<number | null>(null);

  if (isPending) {
    return (
      <div className="grid gap-4 p-4 md:p-6" aria-busy="true">
        <Skeleton className="h-8 w-48" />
        <Skeleton className="h-4 w-72" />
        <div className="grid gap-4 md:grid-cols-2">
          {Array.from({ length: 4 }).map((_, i) => (
            <Skeleton key={i} className="h-40" />
          ))}
        </div>
      </div>
    );
  }

  const selected: InstanceSettings | undefined =
    list.length === 0
      ? undefined
      : list.length === 1
        ? list[0]
        : (list.find((s) => s.id === selectedId) ?? list[0]);

  return (
    <div className="grid gap-4 p-4 md:p-6">
      <div>
        <h1 className="text-xl font-semibold">{t("title")}</h1>
        <p className="text-sm text-muted-foreground">{t("subtitle")}</p>
      </div>

      {list.length > 1 && (
        <div className="max-w-xs">
          <Select
            value={String(selected?.id ?? list[0].id)}
            onValueChange={(v) => setSelectedId(Number(v))}
          >
            <SelectTrigger>
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {list.map((s) => (
                <SelectItem key={s.id} value={String(s.id)}>
                  {s.instance_name}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>
      )}

      {list.length === 0 && (
        <p className="text-sm text-muted-foreground">{t("empty")}</p>
      )}

      <SettingsForm key={selected?.id ?? "new"} settings={selected} />
    </div>
  );
}
