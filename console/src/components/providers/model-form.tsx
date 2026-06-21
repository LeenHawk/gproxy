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

const PRICE_KEYS = ["input", "output", "cache_read", "cache_creation", "image"] as const;
type PriceKey = (typeof PRICE_KEYS)[number];

function readPrice(pricing: unknown, key: PriceKey): string {
  if (pricing && typeof pricing === "object" && key in (pricing as Record<string, unknown>)) {
    const v = (pricing as Record<string, unknown>)[key];
    if (typeof v === "string" || typeof v === "number") return String(v);
  }
  return "";
}

/** Build pricing_json from the 5 simple fields, preserving original keys this flat
 *  editor can't represent (e.g. a tiered `image` object, or any future key), so an
 *  unrelated edit never silently drops them. null when nothing remains. */
function buildPricing(prices: Record<PriceKey, string>, original: unknown): Record<string, unknown> | null {
  const out: Record<string, unknown> = {};
  if (original && typeof original === "object" && !Array.isArray(original)) {
    for (const [k, v] of Object.entries(original as Record<string, unknown>)) {
      const preservable = !(PRICE_KEYS as readonly string[]).includes(k) || (v !== null && typeof v === "object");
      if (preservable) out[k] = v;
    }
  }
  for (const k of PRICE_KEYS) {
    if (prices[k].trim() !== "") out[k] = prices[k].trim();
  }
  return Object.keys(out).length > 0 ? out : null;
}

/** A non-scalar `image` price (tiered object) can't be edited in the flat field. */
function imageIsTiered(pricing: unknown): boolean {
  if (!pricing || typeof pricing !== "object") return false;
  const v = (pricing as Record<string, unknown>).image;
  return v !== null && typeof v === "object";
}

function readVariants(variants: unknown): { suffixes: string; exposeBase: boolean } {
  if (Array.isArray(variants)) return { suffixes: variants.join("\n"), exposeBase: true };
  if (variants && typeof variants === "object") {
    const o = variants as { suffixes?: unknown; expose_base?: unknown };
    const arr = Array.isArray(o.suffixes) ? o.suffixes.map(String) : [];
    return { suffixes: arr.join("\n"), exposeBase: o.expose_base !== false };
  }
  return { suffixes: "", exposeBase: true };
}

/** null when no suffixes; array form when exposeBase; object form when hiding base. */
function buildVariants(suffixesText: string, exposeBase: boolean): unknown {
  const suffixes = suffixesText.split("\n").map((s) => s.trim()).filter((s) => s !== "");
  if (suffixes.length === 0) return null;
  return exposeBase ? suffixes : { expose_base: false, suffixes };
}

export function ModelForm({ providerId, model, onSaved }: { providerId: number; model?: ProviderModel; onSaved: () => void }) {
  const { t } = useTranslation("providers");
  const queryClient = useQueryClient();
  const editing = model !== undefined;
  const imageTiered = imageIsTiered(model?.pricing_json);

  const [modelId, setModelId] = useState(model?.model_id ?? "");
  const [displayName, setDisplayName] = useState(model?.display_name ?? "");
  const [enabled, setEnabled] = useState(model?.enabled ?? true);
  const [prices, setPrices] = useState<Record<PriceKey, string>>(() =>
    Object.fromEntries(PRICE_KEYS.map((k) => [k, readPrice(model?.pricing_json, k)])) as Record<PriceKey, string>,
  );
  const initVariants = readVariants(model?.variants_json);
  const [suffixes, setSuffixes] = useState(initVariants.suffixes);
  const [exposeBase, setExposeBase] = useState(initVariants.exposeBase);
  const [formError, setFormError] = useState<string | null>(null);

  const mutation = useMutation({
    mutationFn: () => {
      if (!modelId.trim()) throw new ApiError(0, "bad_request", t("form.required"));
      const pricing = buildPricing(prices, model?.pricing_json);
      const variants = buildVariants(suffixes, exposeBase);
      return upsertProviderModel(providerId, {
        id: model?.id ?? null,
        provider_id: providerId,
        model_id: modelId.trim(),
        display_name: displayName.trim() === "" ? null : displayName.trim(),
        // pricing_json is REQUIRED in the body (no serde default) — always send it,
        // null when blank (harmless for billing). variants_json HAS serde default → omit.
        pricing_json: pricing,
        ...(variants !== null ? { variants_json: variants } : {}),
        enabled,
      });
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["providers", providerId, "models"] });
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

      <fieldset className="grid gap-3 rounded-md border p-3">
        <legend className="px-1 text-sm font-medium">{t("models.pricing")}</legend>
        <div className="grid grid-cols-2 gap-3">
          {PRICE_KEYS.map((k) => {
            const tieredImage = k === "image" && imageTiered;
            return (
              <div key={k} className="grid gap-1">
                <Label htmlFor={`md-price-${k}`} className="text-xs">{t(`models.price.${k}`)}</Label>
                <Input
                  id={`md-price-${k}`}
                  inputMode="decimal"
                  value={prices[k]}
                  disabled={tieredImage}
                  placeholder={tieredImage ? t("models.imageTiered") : "0"}
                  onChange={(e) => setPrices((p) => ({ ...p, [k]: e.target.value }))}
                />
              </div>
            );
          })}
        </div>
        <p className="text-xs text-muted-foreground">{t("models.pricingHint")}</p>
      </fieldset>

      <fieldset className="grid gap-2 rounded-md border p-3">
        <legend className="px-1 text-sm font-medium">{t("models.variants")}</legend>
        <textarea
          aria-label={t("models.variantsHint")}
          className="min-h-20 rounded-md border bg-transparent p-2 font-mono text-xs"
          value={suffixes} spellCheck={false}
          onChange={(e) => setSuffixes(e.target.value)} placeholder={"-thinking\n-32k"}
        />
        <label className="flex items-center gap-2 text-sm">
          <Switch checked={exposeBase} onCheckedChange={setExposeBase} />
          {t("models.exposeBase")}
        </label>
        <p className="text-xs text-muted-foreground">{t("models.variantsHint")}</p>
      </fieldset>

      <div className="flex items-center justify-between">
        <Label htmlFor="md-enabled">{t("models.enabled")}</Label>
        <Switch id="md-enabled" checked={enabled} onCheckedChange={setEnabled} />
      </div>
      {formError && <p className="text-sm text-destructive">{formError}</p>}
      <Button type="submit" disabled={mutation.isPending}>{editing ? t("models.edit") : t("models.add")}</Button>
    </form>
  );
}
