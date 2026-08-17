import { useEffect, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { upsertProviderModel, type ProviderModel } from "@/api/provider-models";
import { ApiError } from "@/api/http";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { VariantEditor, type VariantRow } from "@/components/providers/variant-editor";
import { ModelThinkingFields } from "@/components/providers/model-thinking-fields";
import { loadVariantActions, syncModelVariants, parseVariantNames } from "@/lib/variant-sync";
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

function positiveInteger(value: string, message: string): number | null {
  const trimmed = value.trim();
  if (trimmed === "") return null;
  const parsed = Number(trimmed);
  if (!Number.isSafeInteger(parsed) || parsed <= 0) {
    throw new ApiError(0, "bad_request", message);
  }
  return parsed;
}

export function ModelForm({ providerId, providerName, channel, model, onSaved }: { providerId: number; providerName: string; channel: string; model?: ProviderModel; onSaved: () => void }) {
  const { t } = useTranslation("providers");
  const queryClient = useQueryClient();
  const editing = model !== undefined;

  const [modelId, setModelId] = useState(model?.model_id ?? "");
  const [displayName, setDisplayName] = useState(model?.display_name ?? "");
  const [contextWindow, setContextWindow] = useState(String(model?.context_window ?? ""));
  const [maxOutputTokens, setMaxOutputTokens] = useState(String(model?.max_output_tokens ?? ""));
  const [thinkingSupported, setThinkingSupported] = useState<boolean | null>(model?.thinking_supported ?? null);
  const [thinkingAdaptive, setThinkingAdaptive] = useState<boolean | null>(model?.thinking_adaptive_supported ?? null);
  const [thinkingEnabled, setThinkingEnabled] = useState<boolean | null>(model?.thinking_enabled_supported ?? null);
  const [enabled, setEnabled] = useState(model?.enabled ?? true);
  const initVariants = readVariantRows(model?.variants_json);
  const [variantRows, setVariantRows] = useState<VariantRow[]>(initVariants.rows);
  const [exposeBase, setExposeBase] = useState(initVariants.exposeBase);
  const [formError, setFormError] = useState<string | null>(null);

  const [oldNames] = useState(() => parseVariantNames(model?.variants_json));
  const behaviorQuery = useQuery({
    queryKey: ["providers", providerId, "variant-actions", model?.id ?? "new"],
    queryFn: () => loadVariantActions(providerId, oldNames),
    enabled: editing && oldNames.length > 0,
  });

  useEffect(() => {
    if (!behaviorQuery.data) return;
    setVariantRows((rows) => rows.map((row) => {
      if (row.touched) return row;
      const actions = behaviorQuery.data.get(row.name);
      return actions ? { ...row, actions } : row;
    }));
  }, [behaviorQuery.data]);

  const mutation = useMutation({
    mutationFn: async () => {
      if (!modelId.trim()) throw new ApiError(0, "bad_request", t("form.required"));
      const variants = buildVariantsJson(variantRows, exposeBase);
      const invalidLimit = t("models.limitInvalid");
      const parsedContextWindow = positiveInteger(contextWindow, invalidLimit);
      const parsedMaxOutputTokens = positiveInteger(maxOutputTokens, invalidLimit);
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
        context_window: parsedContextWindow,
        max_output_tokens: parsedMaxOutputTokens,
        thinking_supported: thinkingSupported,
        thinking_adaptive_supported: thinkingAdaptive,
        thinking_enabled_supported: thinkingEnabled,
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
      void queryClient.invalidateQueries({ queryKey: ["providers", providerId, "variant-actions"] });
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

      <div className="grid gap-3 sm:grid-cols-2">
        <div className="grid gap-2">
          <Label htmlFor="md-context-window">{t("models.contextWindow")}</Label>
          <Input id="md-context-window" type="number" min={1} step={1} value={contextWindow} onChange={(e) => setContextWindow(e.target.value)} />
        </div>
        <div className="grid gap-2">
          <Label htmlFor="md-max-output">{t("models.maxOutputTokens")}</Label>
          <Input id="md-max-output" type="number" min={1} step={1} value={maxOutputTokens} onChange={(e) => setMaxOutputTokens(e.target.value)} />
        </div>
      </div>
      <p className="text-xs text-muted-foreground">{t("models.limitsHint")}</p>


      <ModelThinkingFields
        supported={thinkingSupported}
        adaptive={thinkingAdaptive}
        enabled={thinkingEnabled}
        onSupportedChange={setThinkingSupported}
        onAdaptiveChange={setThinkingAdaptive}
        onEnabledChange={setThinkingEnabled}
      />

      <VariantEditor
        rows={variantRows}
        exposeBase={exposeBase}
        modelId={modelId}
        channel={channel}
        behaviorsLoading={behaviorQuery.isFetching}
        onChange={setVariantRows}
        onExposeBaseChange={setExposeBase}
      />

      <div className="flex items-center justify-between">
        <Label htmlFor="md-enabled">{t("models.enabled")}</Label>
        <Switch id="md-enabled" checked={enabled} onCheckedChange={setEnabled} />
      </div>
      {(formError || behaviorQuery.error) && (
        <p className="text-sm text-destructive">
          {formError ?? (behaviorQuery.error instanceof ApiError ? behaviorQuery.error.message : String(behaviorQuery.error))}
        </p>
      )}
      <Button type="submit" disabled={mutation.isPending || behaviorQuery.isFetching || behaviorQuery.isError}>
        {editing ? t("models.edit") : t("models.add")}
      </Button>
    </form>
  );
}
