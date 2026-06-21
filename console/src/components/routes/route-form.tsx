import { useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { upsertRoute, ROUTE_STRATEGIES, type Route } from "@/api/routes";
import { ApiError } from "@/api/http";
import { JsonField, parseJsonText } from "@/components/form/json-field";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Select, SelectContent, SelectItem, SelectTrigger, SelectValue,
} from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";

export function RouteForm({ route, onSaved }: { route?: Route; onSaved: (saved: Route) => void }) {
  const { t } = useTranslation("routes");
  const { t: tc } = useTranslation("common"); // json.invalid lives in common
  const queryClient = useQueryClient();
  const editing = route !== undefined;

  const [name, setName] = useState(route?.name ?? "");
  const [strategy, setStrategy] = useState(route?.strategy ?? "failover");
  const [description, setDescription] = useState(route?.description ?? "");
  const [enabled, setEnabled] = useState(route?.enabled ?? true);
  const [settingsText, setSettingsText] = useState(
    route?.settings_json == null ? "" : JSON.stringify(route.settings_json, null, 2),
  );
  const [formError, setFormError] = useState<string | null>(null);

  const mutation = useMutation({
    mutationFn: () => {
      if (!name.trim()) throw new ApiError(0, "bad_request", t("form.required"));
      const settings = settingsText.trim() === "" ? { ok: true as const, value: null } : parseJsonText(settingsText);
      if (!settings.ok) throw new ApiError(0, "bad_request", tc("json.invalid"));
      return upsertRoute({
        id: route?.id ?? null,
        name: name.trim(),
        strategy,
        enabled,
        description: description.trim() === "" ? null : description.trim(),
        ...(settings.value !== null ? { settings_json: settings.value } : {}),
      });
    },
    onSuccess: (saved) => {
      void queryClient.invalidateQueries({ queryKey: ["routes"] });
      toast.success(t("form.saved"));
      onSaved(saved);
    },
    onError: (error) => setFormError(error instanceof ApiError ? error.message : String(error)),
  });

  return (
    <form className="grid gap-4" onSubmit={(e) => { e.preventDefault(); setFormError(null); mutation.mutate(); }}>
      <div className="grid gap-2">
        <Label htmlFor="r-name">{t("fields.name")}</Label>
        <Input id="r-name" value={name} onChange={(e) => setName(e.target.value)} required />
      </div>
      <div className="grid gap-2">
        <Label>{t("fields.strategy")}</Label>
        <Select value={strategy} onValueChange={setStrategy}>
          <SelectTrigger><SelectValue /></SelectTrigger>
          <SelectContent>
            {ROUTE_STRATEGIES.map((s) => (
              <SelectItem key={s} value={s}>{t(`strategy.${s}`)}</SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>
      <div className="grid gap-2">
        <Label htmlFor="r-desc">{t("fields.description")}</Label>
        <Input id="r-desc" value={description} onChange={(e) => setDescription(e.target.value)} />
      </div>
      <div className="grid gap-2">
        <Label htmlFor="r-settings">{t("fields.settings")}</Label>
        <JsonField id="r-settings" value={settingsText} onChange={setSettingsText} rows={4} hint={t("form.settingsHint")} />
      </div>
      <div className="flex items-center justify-between">
        <Label htmlFor="r-enabled">{t("fields.enabled")}</Label>
        <Switch id="r-enabled" checked={enabled} onCheckedChange={setEnabled} />
      </div>
      {formError && <p className="text-sm text-destructive">{formError}</p>}
      <Button type="submit" disabled={mutation.isPending}>
        {editing ? t("form.edit") : t("form.create")}
      </Button>
    </form>
  );
}
