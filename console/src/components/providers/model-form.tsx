import { useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { upsertProviderModel, type ProviderModel } from "@/api/provider-models";
import { ApiError } from "@/api/http";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { VariantEditor, type VariantRow } from "@/components/providers/variant-editor";
import { syncModelVariants, parseVariantNames } from "@/lib/variant-sync";
import type { SuffixAction } from "@/components/providers/suffix-presets";

function readVariantRows(variants: unknown): { rows: VariantRow[]; exposeBase: boolean } {
  const names = parseVariantNames(variants);
  let exposeBase = true;
  if (variants && typeof variants === "object" && !Array.isArray(variants)) {
    exposeBase = (variants as { expose_base?: unknown }).expose_base !== false;
  }
  return { rows: names.map((name) => ({ name, actions: [], touched: false })), exposeBase };
}

/** null when no names; bare array when exposeBase; object form when hiding base. */
function buildVariantsJson(rows: VariantRow[], exposeBase: boolean): unknown {
  const names = rows.map((r) => r.name.trim()).filter((n) => n !== "");
  if (names.length === 0) return null;
  return exposeBase ? names : { expose_base: false, variants: names };
}

export function ModelForm({ providerId, providerName, channel, model, onSaved }: { providerId: number; providerName: string; channel: string; model?: ProviderModel; onSaved: () => void }) {
  const { t } = useTranslation("providers");
  const queryClient = useQueryClient();
  const editing = model !== undefined;

  const [modelId, setModelId] = useState(model?.model_id ?? "");
  const [displayName, setDisplayName] = useState(model?.display_name ?? "");
  const [enabled, setEnabled] = useState(model?.enabled ?? true);
  const initVariants = readVariantRows(model?.variants_json);
  const [variantRows, setVariantRows] = useState<VariantRow[]>(initVariants.rows);
  const [exposeBase, setExposeBase] = useState(initVariants.exposeBase);
  const [formError, setFormError] = useState<string | null>(null);

  const [oldNames] = useState(() => parseVariantNames(model?.variants_json));

  const mutation = useMutation({
    mutationFn: async () => {
      if (!modelId.trim()) throw new ApiError(0, "bad_request", t("form.required"));
      const variants = buildVariantsJson(variantRows, exposeBase);
      const newNames = variantRows.map((r) => r.name.trim()).filter((n) => n !== "");
      const presetActions = new Map<string, SuffixAction[]>();
      for (const r of variantRows) {
        const n = r.name.trim();
        if (r.touched && n !== "") presetActions.set(n, r.actions);
      }
      const saved = await upsertProviderModel(providerId, {
        id: model?.id ?? null,
        provider_id: providerId,
        model_id: modelId.trim(),
        display_name: displayName.trim() === "" ? null : displayName.trim(),
        ...(variants !== null ? { variants_json: variants } : {}),
        enabled,
      });
      await syncModelVariants({
        providerId,
        providerName,
        oldNames,
        newNames,
        presetActions,
      });
      return saved;
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["providers", providerId, "models"] });
      void queryClient.invalidateQueries({ queryKey: ["rule-sets"] });
      toast.success(t("form.saved"));
      onSaved();
    },
    onError: (error) => setFormError(error instanceof ApiError ? error.message : String(error)),
  });

  return (
    <form className="grid gap-4" onSubmit={(e) => { e.preventDefault(); setFormError(null); mutation.mutate(); }}>
      <div className="grid gap-2">
        <Label htmlFor="md-id">{t("models.modelId")}</Label>
        <Input id="md-id" value={modelId} onChange={(e) => setModelId(e.target.value)} required />
        <p className="text-xs text-muted-foreground">{t("models.modelIdHint")}</p>
      </div>
      <div className="grid gap-2">
        <Label htmlFor="md-name">{t("models.displayName")}</Label>
        <Input id="md-name" value={displayName} onChange={(e) => setDisplayName(e.target.value)} />
      </div>

      <VariantEditor
        rows={variantRows}
        exposeBase={exposeBase}
        modelId={modelId}
        channel={channel}
        onChange={setVariantRows}
        onExposeBaseChange={setExposeBase}
      />

      <div className="flex items-center justify-between">
        <Label htmlFor="md-enabled">{t("models.enabled")}</Label>
        <Switch id="md-enabled" checked={enabled} onCheckedChange={setEnabled} />
      </div>
      {formError && <p className="text-sm text-destructive">{formError}</p>}
      <Button type="submit" disabled={mutation.isPending}>{editing ? t("models.edit") : t("models.add")}</Button>
    </form>
  );
}
