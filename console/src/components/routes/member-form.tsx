import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { upsertRouteMember, type RouteMember } from "@/api/routes";
import { providersQuery } from "@/api/providers";
import { providerModelsQuery } from "@/api/provider-models";
import { ApiError } from "@/api/http";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";

function intOr(v: string, fallback: number): number {
  const n = Number(v);
  return v.trim() !== "" && Number.isInteger(n) ? n : fallback;
}

export function MemberForm({
  routeId,
  member,
  onSaved,
}: {
  routeId: number;
  member?: RouteMember;
  onSaved: () => void;
}) {
  const { t } = useTranslation("routes");
  const queryClient = useQueryClient();
  const editing = member !== undefined;
  const { data: providers } = useQuery(providersQuery);

  const [providerId, setProviderId] = useState<number | null>(member?.provider_id ?? null);
  const [model, setModel] = useState(member?.upstream_model_id ?? "");
  const [weight, setWeight] = useState(String(member?.weight ?? 100));
  const [tier, setTier] = useState(String(member?.tier ?? 0));
  const [enabled, setEnabled] = useState(member?.enabled ?? true);
  const [formError, setFormError] = useState<string | null>(null);

  // Model suggestions from the selected provider's registered models (datalist).
  const { data: models } = useQuery({
    ...providerModelsQuery(providerId ?? 0),
    enabled: providerId !== null,
  });

  const mutation = useMutation({
    mutationFn: () => {
      if (providerId === null) throw new ApiError(0, "bad_request", t("form.required"));
      if (!model.trim()) throw new ApiError(0, "bad_request", t("form.required"));
      return upsertRouteMember(routeId, {
        id: member?.id ?? null,
        route_id: routeId,
        provider_id: providerId,
        upstream_model_id: model.trim(),
        weight: intOr(weight, 100),
        tier: intOr(tier, 0),
        enabled,
      });
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["routes", routeId, "members"] });
      toast.success(t("form.saved"));
      onSaved();
    },
    onError: (error) => setFormError(error instanceof ApiError ? error.message : String(error)),
  });

  return (
    <form
      className="grid gap-4"
      onSubmit={(e) => {
        e.preventDefault();
        setFormError(null);
        mutation.mutate();
      }}
    >
      <div className="grid gap-2">
        <Label>{t("members.provider")}</Label>
        <Select
          value={providerId === null ? "" : String(providerId)}
          onValueChange={(v) => {
            setProviderId(Number(v));
            setModel("");
          }}
        >
          <SelectTrigger>
            <SelectValue placeholder="—" />
          </SelectTrigger>
          <SelectContent>
            {(providers ?? []).map((p) => (
              <SelectItem key={p.id} value={String(p.id)}>
                {(p.label ?? p.name)} · {p.channel}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>
      <div className="grid gap-2">
        <Label htmlFor="m-model">{t("members.model")}</Label>
        <Input
          id="m-model"
          list="m-model-list"
          value={model}
          onChange={(e) => setModel(e.target.value)}
          placeholder="gpt-4o / claude-3-5-sonnet-…"
        />
        <datalist id="m-model-list">
          {(models ?? []).map((md) => (
            <option key={md.id} value={md.model_id} />
          ))}
        </datalist>
        <p className="text-xs text-muted-foreground">{t("members.modelHint")}</p>
      </div>
      <div className="grid grid-cols-2 gap-4">
        <div className="grid gap-2">
          <Label htmlFor="m-tier">{t("members.tier")}</Label>
          <Input
            id="m-tier"
            inputMode="numeric"
            value={tier}
            onChange={(e) => setTier(e.target.value)}
          />
          <p className="text-xs text-muted-foreground">{t("members.tierHint")}</p>
        </div>
        <div className="grid gap-2">
          <Label htmlFor="m-weight">{t("members.weight")}</Label>
          <Input
            id="m-weight"
            inputMode="numeric"
            value={weight}
            onChange={(e) => setWeight(e.target.value)}
          />
          <p className="text-xs text-muted-foreground">{t("members.weightHint")}</p>
        </div>
      </div>
      <div className="flex items-center justify-between">
        <Label htmlFor="m-enabled">{t("fields.enabled")}</Label>
        <Switch id="m-enabled" checked={enabled} onCheckedChange={setEnabled} />
      </div>
      {formError && <p className="text-sm text-destructive">{formError}</p>}
      <Button type="submit" disabled={mutation.isPending}>
        {editing ? t("members.edit") : t("members.add")}
      </Button>
    </form>
  );
}
