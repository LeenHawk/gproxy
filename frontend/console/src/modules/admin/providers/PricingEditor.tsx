import { useEffect, useMemo, useState } from "react";

import { Button, Input, Label, TextArea } from "../../../components/ui";

/// Matches `gproxy_sdk::provider::billing::ModelPriceTier`, with all numeric
/// fields held as strings so the form can represent an intermediate "" state
/// while the user is typing.
type TierForm = {
  input_tokens_up_to: string;
  price_input_tokens: string;
  price_output_tokens: string;
  price_cache_read_input_tokens: string;
  price_cache_creation_input_tokens: string;
  price_cache_creation_input_tokens_5min: string;
  price_cache_creation_input_tokens_1h: string;
};

const EMPTY_TIER: TierForm = {
  input_tokens_up_to: "",
  price_input_tokens: "",
  price_output_tokens: "",
  price_cache_read_input_tokens: "",
  price_cache_creation_input_tokens: "",
  price_cache_creation_input_tokens_5min: "",
  price_cache_creation_input_tokens_1h: "",
};

type ToolPriceForm = { key: string; price: string };

type PricingForm = {
  price_each_call: string;
  price_tiers: TierForm[];
  flex_price_each_call: string;
  flex_price_tiers: TierForm[];
  scale_price_each_call: string;
  scale_price_tiers: TierForm[];
  priority_price_each_call: string;
  priority_price_tiers: TierForm[];
  tool_call_prices: ToolPriceForm[];
};

const EMPTY_FORM: PricingForm = {
  price_each_call: "",
  price_tiers: [],
  flex_price_each_call: "",
  flex_price_tiers: [],
  scale_price_each_call: "",
  scale_price_tiers: [],
  priority_price_each_call: "",
  priority_price_tiers: [],
  tool_call_prices: [],
};

function parseJsonToForm(raw: string): PricingForm | null {
  if (!raw.trim()) return { ...EMPTY_FORM };
  try {
    const obj = JSON.parse(raw);
    if (obj === null || typeof obj !== "object") return null;
    const num = (v: unknown): string =>
      typeof v === "number" ? String(v) : "";
    const tier = (t: unknown): TierForm => {
      const src = (t ?? {}) as Record<string, unknown>;
      return {
        input_tokens_up_to: num(src.input_tokens_up_to),
        price_input_tokens: num(src.price_input_tokens),
        price_output_tokens: num(src.price_output_tokens),
        price_cache_read_input_tokens: num(src.price_cache_read_input_tokens),
        price_cache_creation_input_tokens: num(src.price_cache_creation_input_tokens),
        price_cache_creation_input_tokens_5min: num(src.price_cache_creation_input_tokens_5min),
        price_cache_creation_input_tokens_1h: num(src.price_cache_creation_input_tokens_1h),
      };
    };
    const tiers = (arr: unknown): TierForm[] =>
      Array.isArray(arr) ? arr.map(tier) : [];
    const toolPrices = (v: unknown): ToolPriceForm[] => {
      if (v === null || typeof v !== "object") return [];
      return Object.entries(v as Record<string, unknown>).map(([key, price]) => ({
        key,
        price: typeof price === "number" ? String(price) : "",
      }));
    };
    return {
      price_each_call: num((obj as Record<string, unknown>).price_each_call),
      price_tiers: tiers((obj as Record<string, unknown>).price_tiers),
      flex_price_each_call: num((obj as Record<string, unknown>).flex_price_each_call),
      flex_price_tiers: tiers((obj as Record<string, unknown>).flex_price_tiers),
      scale_price_each_call: num((obj as Record<string, unknown>).scale_price_each_call),
      scale_price_tiers: tiers((obj as Record<string, unknown>).scale_price_tiers),
      priority_price_each_call: num((obj as Record<string, unknown>).priority_price_each_call),
      priority_price_tiers: tiers((obj as Record<string, unknown>).priority_price_tiers),
      tool_call_prices: toolPrices((obj as Record<string, unknown>).tool_call_prices),
    };
  } catch {
    return null;
  }
}

function numOrNull(s: string): number | null {
  const trimmed = s.trim();
  if (!trimmed) return null;
  const n = Number(trimmed);
  return Number.isFinite(n) ? n : null;
}

function tierToJson(t: TierForm): Record<string, unknown> | null {
  const upTo = numOrNull(t.input_tokens_up_to);
  if (upTo === null) return null; // tier without an upper bound is invalid
  const out: Record<string, unknown> = { input_tokens_up_to: upTo };
  const fields: Array<[keyof TierForm, string]> = [
    ["price_input_tokens", "price_input_tokens"],
    ["price_output_tokens", "price_output_tokens"],
    ["price_cache_read_input_tokens", "price_cache_read_input_tokens"],
    ["price_cache_creation_input_tokens", "price_cache_creation_input_tokens"],
    ["price_cache_creation_input_tokens_5min", "price_cache_creation_input_tokens_5min"],
    ["price_cache_creation_input_tokens_1h", "price_cache_creation_input_tokens_1h"],
  ];
  for (const [src, dst] of fields) {
    const n = numOrNull(t[src]);
    if (n !== null) out[dst] = n;
  }
  return out;
}

function formToJson(form: PricingForm): string {
  const out: Record<string, unknown> = {};
  const putNumber = (key: string, raw: string) => {
    const n = numOrNull(raw);
    if (n !== null) out[key] = n;
  };
  const putTiers = (key: string, arr: TierForm[]) => {
    const mapped = arr.map(tierToJson).filter((t): t is Record<string, unknown> => t !== null);
    if (mapped.length > 0) out[key] = mapped;
  };
  putNumber("price_each_call", form.price_each_call);
  putTiers("price_tiers", form.price_tiers);
  putNumber("flex_price_each_call", form.flex_price_each_call);
  putTiers("flex_price_tiers", form.flex_price_tiers);
  putNumber("scale_price_each_call", form.scale_price_each_call);
  putTiers("scale_price_tiers", form.scale_price_tiers);
  putNumber("priority_price_each_call", form.priority_price_each_call);
  putTiers("priority_price_tiers", form.priority_price_tiers);
  const toolPrices: Record<string, number> = {};
  for (const { key, price } of form.tool_call_prices) {
    const k = key.trim();
    const n = numOrNull(price);
    if (k && n !== null) toolPrices[k] = n;
  }
  if (Object.keys(toolPrices).length > 0) out.tool_call_prices = toolPrices;
  if (Object.keys(out).length === 0) return "";
  return JSON.stringify(out, null, 2);
}

export type PricingEditorLabels = {
  modeStructured: string;
  modeJson: string;
  priceEachCall: string;
  priceTiers: string;
  flexPriceEachCall: string;
  flexPriceTiers: string;
  scalePriceEachCall: string;
  scalePriceTiers: string;
  priorityPriceEachCall: string;
  priorityPriceTiers: string;
  toolCallPrices: string;
  addTier: string;
  addToolPrice: string;
  removeRow: string;
  tierInputTokensUpTo: string;
  tierPriceInput: string;
  tierPriceOutput: string;
  tierPriceCacheRead: string;
  tierPriceCacheCreation: string;
  tierPriceCacheCreation5min: string;
  tierPriceCacheCreation1h: string;
  toolKey: string;
  toolPrice: string;
  emptyHint: string;
  jsonParseError: string;
  jsonTextareaLabel: string;
};

export function PricingEditor({
  value,
  onChange,
  labels,
}: {
  value: string;
  onChange: (newJson: string) => void;
  labels: PricingEditorLabels;
}) {
  const initialForm = useMemo(() => parseJsonToForm(value), [value]);
  const parsable = initialForm !== null;
  const [mode, setMode] = useState<"structured" | "json">(
    parsable ? "structured" : "json",
  );
  const [form, setForm] = useState<PricingForm>(initialForm ?? EMPTY_FORM);
  const [lastParsedFrom, setLastParsedFrom] = useState(value);

  // Sync structured state when an outside change to `value` happens
  // (e.g. the parent reloaded a different model row).
  useEffect(() => {
    if (value === lastParsedFrom) return;
    const parsed = parseJsonToForm(value);
    if (parsed !== null) {
      setForm(parsed);
      setMode("structured");
    } else {
      setMode("json");
    }
    setLastParsedFrom(value);
  }, [value, lastParsedFrom]);

  const commit = (next: PricingForm) => {
    setForm(next);
    const json = formToJson(next);
    setLastParsedFrom(json);
    onChange(json);
  };

  const updateTier = (
    tiersKey:
      | "price_tiers"
      | "flex_price_tiers"
      | "scale_price_tiers"
      | "priority_price_tiers",
    index: number,
    field: keyof TierForm,
    value: string,
  ) => {
    const nextTiers = form[tiersKey].map((t, i) =>
      i === index ? { ...t, [field]: value } : t,
    );
    commit({ ...form, [tiersKey]: nextTiers });
  };

  const addTier = (
    tiersKey:
      | "price_tiers"
      | "flex_price_tiers"
      | "scale_price_tiers"
      | "priority_price_tiers",
  ) => {
    commit({ ...form, [tiersKey]: [...form[tiersKey], { ...EMPTY_TIER }] });
  };

  const removeTier = (
    tiersKey:
      | "price_tiers"
      | "flex_price_tiers"
      | "scale_price_tiers"
      | "priority_price_tiers",
    index: number,
  ) => {
    commit({
      ...form,
      [tiersKey]: form[tiersKey].filter((_, i) => i !== index),
    });
  };

  const updateToolPrice = (index: number, patch: Partial<ToolPriceForm>) => {
    const next = form.tool_call_prices.map((t, i) =>
      i === index ? { ...t, ...patch } : t,
    );
    commit({ ...form, tool_call_prices: next });
  };

  const addToolPrice = () => {
    commit({ ...form, tool_call_prices: [...form.tool_call_prices, { key: "", price: "" }] });
  };

  const removeToolPrice = (index: number) => {
    commit({
      ...form,
      tool_call_prices: form.tool_call_prices.filter((_, i) => i !== index),
    });
  };

  return (
    <div className="space-y-3">
      <div className="flex items-center gap-2 text-xs">
        <button
          type="button"
          className={`px-2 py-1 rounded border ${mode === "structured" ? "bg-panel-muted border-border" : "border-transparent text-muted"}`}
          onClick={() => {
            if (mode !== "structured") {
              const parsed = parseJsonToForm(value);
              if (parsed !== null) {
                setForm(parsed);
                setLastParsedFrom(value);
                setMode("structured");
              }
            }
          }}
          disabled={!parsable && mode === "json"}
        >
          {labels.modeStructured}
        </button>
        <button
          type="button"
          className={`px-2 py-1 rounded border ${mode === "json" ? "bg-panel-muted border-border" : "border-transparent text-muted"}`}
          onClick={() => setMode("json")}
        >
          {labels.modeJson}
        </button>
        {!parsable ? (
          <span className="text-danger">{labels.jsonParseError}</span>
        ) : null}
      </div>

      {mode === "json" ? (
        <div>
          <Label>{labels.jsonTextareaLabel}</Label>
          <TextArea
            value={value}
            onChange={(next) => {
              onChange(next);
              setLastParsedFrom(next);
              const parsed = parseJsonToForm(next);
              if (parsed !== null) setForm(parsed);
            }}
            rows={12}
          />
        </div>
      ) : (
        <div className="space-y-4 text-sm">
          <div>
            <Label>{labels.priceEachCall}</Label>
            <Input
              value={form.price_each_call}
              onChange={(v) => commit({ ...form, price_each_call: v })}
              placeholder="0.0"
            />
          </div>

          <TierSection
            label={labels.priceTiers}
            tiers={form.price_tiers}
            labels={labels}
            onUpdate={(i, f, v) => updateTier("price_tiers", i, f, v)}
            onAdd={() => addTier("price_tiers")}
            onRemove={(i) => removeTier("price_tiers", i)}
          />

          <EachCallAndTiers
            label={labels.flexPriceEachCall}
            tiersLabel={labels.flexPriceTiers}
            eachCall={form.flex_price_each_call}
            onEachCallChange={(v) => commit({ ...form, flex_price_each_call: v })}
            tiers={form.flex_price_tiers}
            labels={labels}
            onUpdate={(i, f, v) => updateTier("flex_price_tiers", i, f, v)}
            onAdd={() => addTier("flex_price_tiers")}
            onRemove={(i) => removeTier("flex_price_tiers", i)}
          />
          <EachCallAndTiers
            label={labels.scalePriceEachCall}
            tiersLabel={labels.scalePriceTiers}
            eachCall={form.scale_price_each_call}
            onEachCallChange={(v) => commit({ ...form, scale_price_each_call: v })}
            tiers={form.scale_price_tiers}
            labels={labels}
            onUpdate={(i, f, v) => updateTier("scale_price_tiers", i, f, v)}
            onAdd={() => addTier("scale_price_tiers")}
            onRemove={(i) => removeTier("scale_price_tiers", i)}
          />
          <EachCallAndTiers
            label={labels.priorityPriceEachCall}
            tiersLabel={labels.priorityPriceTiers}
            eachCall={form.priority_price_each_call}
            onEachCallChange={(v) => commit({ ...form, priority_price_each_call: v })}
            tiers={form.priority_price_tiers}
            labels={labels}
            onUpdate={(i, f, v) => updateTier("priority_price_tiers", i, f, v)}
            onAdd={() => addTier("priority_price_tiers")}
            onRemove={(i) => removeTier("priority_price_tiers", i)}
          />

          <div>
            <div className="flex items-center justify-between">
              <Label>{labels.toolCallPrices}</Label>
              <Button variant="neutral" onClick={addToolPrice}>
                + {labels.addToolPrice}
              </Button>
            </div>
            {form.tool_call_prices.length === 0 ? (
              <p className="text-xs text-muted mt-1">{labels.emptyHint}</p>
            ) : (
              <div className="space-y-1 mt-1">
                {form.tool_call_prices.map((tp, i) => (
                  <div key={i} className="flex items-center gap-2">
                    <div className="flex-1">
                      <Input
                        value={tp.key}
                        onChange={(v) => updateToolPrice(i, { key: v })}
                        placeholder={labels.toolKey}
                      />
                    </div>
                    <div className="flex-1">
                      <Input
                        value={tp.price}
                        onChange={(v) => updateToolPrice(i, { price: v })}
                        placeholder={labels.toolPrice}
                      />
                    </div>
                    <Button variant="danger" onClick={() => removeToolPrice(i)}>
                      {labels.removeRow}
                    </Button>
                  </div>
                ))}
              </div>
            )}
          </div>
        </div>
      )}
    </div>
  );
}

function EachCallAndTiers({
  label,
  tiersLabel,
  eachCall,
  onEachCallChange,
  tiers,
  labels,
  onUpdate,
  onAdd,
  onRemove,
}: {
  label: string;
  tiersLabel: string;
  eachCall: string;
  onEachCallChange: (v: string) => void;
  tiers: TierForm[];
  labels: PricingEditorLabels;
  onUpdate: (index: number, field: keyof TierForm, value: string) => void;
  onAdd: () => void;
  onRemove: (index: number) => void;
}) {
  const [open, setOpen] = useState(
    eachCall.trim() !== "" || tiers.length > 0,
  );
  return (
    <details
      open={open}
      onToggle={(e) => setOpen((e.target as HTMLDetailsElement).open)}
      className="rounded border border-border"
    >
      <summary className="cursor-pointer px-2 py-1 text-xs text-muted hover:bg-panel-muted">
        {label.replace(/^\w/, (c) => c.toUpperCase()).replace(/_price_each_call$/, "")}
      </summary>
      <div className="p-2 space-y-3">
        <div>
          <Label>{label}</Label>
          <Input value={eachCall} onChange={onEachCallChange} placeholder="0.0" />
        </div>
        <TierSection
          label={tiersLabel}
          tiers={tiers}
          labels={labels}
          onUpdate={onUpdate}
          onAdd={onAdd}
          onRemove={onRemove}
        />
      </div>
    </details>
  );
}

function TierSection({
  label,
  tiers,
  labels,
  onUpdate,
  onAdd,
  onRemove,
}: {
  label: string;
  tiers: TierForm[];
  labels: PricingEditorLabels;
  onUpdate: (index: number, field: keyof TierForm, value: string) => void;
  onAdd: () => void;
  onRemove: (index: number) => void;
}) {
  return (
    <div>
      <div className="flex items-center justify-between">
        <Label>{label}</Label>
        <Button variant="neutral" onClick={onAdd}>
          + {labels.addTier}
        </Button>
      </div>
      {tiers.length === 0 ? (
        <p className="text-xs text-muted mt-1">{labels.emptyHint}</p>
      ) : (
        <div className="space-y-2 mt-2">
          {tiers.map((tier, index) => (
            <div
              key={index}
              className="rounded border border-border bg-panel-muted p-2 space-y-1 text-xs"
            >
              <div className="grid grid-cols-2 gap-2">
                <TierField
                  label={labels.tierInputTokensUpTo}
                  value={tier.input_tokens_up_to}
                  onChange={(v) => onUpdate(index, "input_tokens_up_to", v)}
                />
                <TierField
                  label={labels.tierPriceInput}
                  value={tier.price_input_tokens}
                  onChange={(v) => onUpdate(index, "price_input_tokens", v)}
                />
                <TierField
                  label={labels.tierPriceOutput}
                  value={tier.price_output_tokens}
                  onChange={(v) => onUpdate(index, "price_output_tokens", v)}
                />
                <TierField
                  label={labels.tierPriceCacheRead}
                  value={tier.price_cache_read_input_tokens}
                  onChange={(v) => onUpdate(index, "price_cache_read_input_tokens", v)}
                />
                <TierField
                  label={labels.tierPriceCacheCreation}
                  value={tier.price_cache_creation_input_tokens}
                  onChange={(v) => onUpdate(index, "price_cache_creation_input_tokens", v)}
                />
                <TierField
                  label={labels.tierPriceCacheCreation5min}
                  value={tier.price_cache_creation_input_tokens_5min}
                  onChange={(v) =>
                    onUpdate(index, "price_cache_creation_input_tokens_5min", v)
                  }
                />
                <TierField
                  label={labels.tierPriceCacheCreation1h}
                  value={tier.price_cache_creation_input_tokens_1h}
                  onChange={(v) =>
                    onUpdate(index, "price_cache_creation_input_tokens_1h", v)
                  }
                />
              </div>
              <div className="flex justify-end">
                <Button variant="danger" onClick={() => onRemove(index)}>
                  {labels.removeRow}
                </Button>
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

function TierField({
  label,
  value,
  onChange,
}: {
  label: string;
  value: string;
  onChange: (v: string) => void;
}) {
  return (
    <div>
      <div className="text-[10px] uppercase tracking-wide text-muted">{label}</div>
      <Input value={value} onChange={onChange} placeholder="—" />
    </div>
  );
}
