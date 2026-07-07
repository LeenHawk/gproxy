import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { providersQuery } from "@/api/providers";
import { upsertPriceRule, type PriceRule } from "@/api/price-rules";
import { ApiError } from "@/api/http";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";

const GLOBAL = "__global__";

function decimalField(value: string | undefined): string {
  return value == null || value.trim() === "" ? "0" : value;
}

function normalizeDecimal(value: string): string {
  const trimmed = value.trim();
  return trimmed === "" ? "0" : trimmed;
}

interface PriceRuleFormProps {
  rule?: PriceRule;
  initialProviderId?: number | null;
  initialModelMatch?: string;
  modelMatchOptions?: string[];
  onModelMatchChange?: (value: string) => void;
  lockedTarget?: boolean;
  onSaved: () => void;
}

export function PriceRuleForm({
  rule,
  initialProviderId,
  initialModelMatch,
  modelMatchOptions = [],
  onModelMatchChange,
  lockedTarget = false,
  onSaved,
}: PriceRuleFormProps) {
  const { t } = useTranslation("pricing");
  const queryClient = useQueryClient();
  const { data: providers = [] } = useQuery(providersQuery);
  const providerValue = rule?.provider_id ?? initialProviderId;
  const [provider, setProvider] = useState(providerValue == null ? GLOBAL : String(providerValue));
  const [matchType, setMatchType] = useState<"exact" | "contains">(rule?.match_type ?? "exact");
  const [modelMatch, setModelMatch] = useState(rule?.model_match ?? initialModelMatch ?? "");
  const [enabled, setEnabled] = useState(rule?.enabled ?? true);
  const [inputPrice, setInputPrice] = useState(() => decimalField(rule?.input_price));
  const [outputPrice, setOutputPrice] = useState(() => decimalField(rule?.output_price));
  const [cacheReadPrice, setCacheReadPrice] = useState(() => decimalField(rule?.cache_read_price));
  const [cacheCreation5mPrice, setCacheCreation5mPrice] = useState(() => decimalField(rule?.cache_creation_5m_price));
  const [cacheCreation1hPrice, setCacheCreation1hPrice] = useState(() => decimalField(rule?.cache_creation_1h_price));
  const [imagePrice, setImagePrice] = useState(() => decimalField(rule?.image_price));
  const [formError, setFormError] = useState<string | null>(null);
  const matchOptions = [...new Set(modelMatchOptions.map((v) => v.trim()).filter((v) => v !== ""))];

  const mutation = useMutation({
    mutationFn: async () => {
      if (!modelMatch.trim()) throw new ApiError(0, "bad_request", t("form.modelRequired"));
      return upsertPriceRule({
        id: rule?.id ?? null,
        provider_id: provider === GLOBAL ? null : Number(provider),
        match_type: matchType,
        model_match: modelMatch.trim(),
        input_price: normalizeDecimal(inputPrice),
        output_price: normalizeDecimal(outputPrice),
        cache_read_price: normalizeDecimal(cacheReadPrice),
        cache_creation_5m_price: normalizeDecimal(cacheCreation5mPrice),
        cache_creation_1h_price: normalizeDecimal(cacheCreation1hPrice),
        image_price: normalizeDecimal(imagePrice),
        enabled,
      });
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["price-rules"] });
      toast.success(t("form.saved"));
      onSaved();
    },
    onError: (error) => setFormError(error instanceof ApiError ? error.message : String(error)),
  });

  return (
    <form className="grid gap-4" onSubmit={(e) => { e.preventDefault(); setFormError(null); mutation.mutate(); }}>
      <div className="grid gap-2">
        <Label>{t("form.scope")}</Label>
        <Select value={provider} onValueChange={setProvider} disabled={lockedTarget}>
          <SelectTrigger className="w-full"><SelectValue /></SelectTrigger>
          <SelectContent>
            <SelectItem value={GLOBAL}>{t("scope.global")}</SelectItem>
            {providers.map((p) => (
              <SelectItem key={p.id} value={String(p.id)}>
                {p.label ?? p.name}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>

      <div className="grid gap-2 md:grid-cols-[160px_1fr]">
        <div className="grid gap-2">
          <Label>{t("form.matchType")}</Label>
          <Select value={matchType} onValueChange={(v) => setMatchType(v as "exact" | "contains")} disabled={lockedTarget}>
            <SelectTrigger className="w-full"><SelectValue /></SelectTrigger>
            <SelectContent>
              <SelectItem value="exact">{t("match.exact")}</SelectItem>
              <SelectItem value="contains">{t("match.contains")}</SelectItem>
            </SelectContent>
          </Select>
        </div>
        <div className="grid gap-2">
          <Label htmlFor="price-model">{t("form.modelMatch")}</Label>
          {matchOptions.length > 0 ? (
            <Select
              value={modelMatch}
              onValueChange={(value) => {
                setModelMatch(value);
                onModelMatchChange?.(value);
              }}
            >
              <SelectTrigger id="price-model" className="w-full"><SelectValue /></SelectTrigger>
              <SelectContent>
                {matchOptions.map((option) => (
                  <SelectItem key={option} value={option}>{option}</SelectItem>
                ))}
              </SelectContent>
            </Select>
          ) : (
            <Input id="price-model" value={modelMatch} onChange={(e) => setModelMatch(e.target.value)} disabled={lockedTarget} required />
          )}
        </div>
      </div>

      <div className="grid gap-3">
        <Label>{t("form.prices")}</Label>
        <div className="grid gap-3 md:grid-cols-2">
          <div className="grid gap-2">
            <Label htmlFor="price-input">{t("form.inputPrice")}</Label>
            <Input id="price-input" inputMode="decimal" value={inputPrice} onChange={(e) => setInputPrice(e.target.value)} />
          </div>
          <div className="grid gap-2">
            <Label htmlFor="price-output">{t("form.outputPrice")}</Label>
            <Input id="price-output" inputMode="decimal" value={outputPrice} onChange={(e) => setOutputPrice(e.target.value)} />
          </div>
          <div className="grid gap-2">
            <Label htmlFor="price-cache-read">{t("form.cacheReadPrice")}</Label>
            <Input id="price-cache-read" inputMode="decimal" value={cacheReadPrice} onChange={(e) => setCacheReadPrice(e.target.value)} />
          </div>
          <div className="grid gap-2">
            <Label htmlFor="price-cache-creation-5m">{t("form.cacheCreation5mPrice")}</Label>
            <Input id="price-cache-creation-5m" inputMode="decimal" value={cacheCreation5mPrice} onChange={(e) => setCacheCreation5mPrice(e.target.value)} />
          </div>
          <div className="grid gap-2">
            <Label htmlFor="price-cache-creation-1h">{t("form.cacheCreation1hPrice")}</Label>
            <Input id="price-cache-creation-1h" inputMode="decimal" value={cacheCreation1hPrice} onChange={(e) => setCacheCreation1hPrice(e.target.value)} />
          </div>
          <div className="grid gap-2">
            <Label htmlFor="price-image">{t("form.imagePrice")}</Label>
            <Input id="price-image" inputMode="decimal" value={imagePrice} onChange={(e) => setImagePrice(e.target.value)} />
          </div>
        </div>
        <p className="text-xs text-muted-foreground">{t("form.pricesHint")}</p>
      </div>

      <div className="flex items-center justify-between gap-3 rounded-md border px-3 py-2">
        <Label htmlFor="price-enabled">{t("form.enabled")}</Label>
        <Switch id="price-enabled" checked={enabled} onCheckedChange={setEnabled} />
      </div>

      {formError && <p className="text-sm text-destructive">{formError}</p>}
      <Button type="submit" disabled={mutation.isPending}>
        {rule ? t("form.update") : t("form.create")}
      </Button>
    </form>
  );
}
