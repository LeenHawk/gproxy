import { useState } from "react";

import { Button, Input } from "../../../../components/ui";
import {
  RECOMMENDED_CACHE_TEMPLATE,
  parseCacheBreakpoints,
  type CacheBreakpointRule,
} from "../channel-constants";
import { CollapsibleSection, type TranslateFn } from "./CollapsibleSection";

export function CacheBreakpointsEditor({
  value,
  onChange,
  t,
}: {
  value: string;
  onChange: (value: string) => void;
  t: TranslateFn;
}) {
  const [expanded, setExpanded] = useState(false);
  const rules = parseCacheBreakpoints(value);
  const slots: Array<CacheBreakpointRule | null> = [
    rules[0] ?? null,
    rules[1] ?? null,
    rules[2] ?? null,
    rules[3] ?? null,
  ];

  const commit = (nextSlots: Array<CacheBreakpointRule | null>) => {
    onChange(JSON.stringify(nextSlots.filter((r): r is CacheBreakpointRule => r !== null)));
  };

  const updateSlot = (idx: number, patch: Partial<CacheBreakpointRule>) => {
    const next = [...slots];
    const current = next[idx] ?? { target: "messages", position: "nth", index: 1, ttl: "auto" };
    next[idx] = { ...current, ...patch };
    commit(next);
  };

  const clearSlot = (idx: number) => {
    const next = [...slots];
    next[idx] = null;
    commit(next);
  };

  // Example cards
  const exampleCards: Array<{ label: string; rule: CacheBreakpointRule }> = [
    { label: t("providers.cacheBreakpoints.example.topLevel"), rule: { target: "top_level", position: "nth", index: 1, ttl: "auto" } },
    { label: t("providers.cacheBreakpoints.example.systemLast"), rule: { target: "system", position: "last_nth", index: 1, ttl: "auto" } },
    { label: t("providers.cacheBreakpoints.example.messagesLast11"), rule: { target: "messages", position: "last_nth", index: 11, ttl: "auto" } },
    { label: t("providers.cacheBreakpoints.example.messagesLast1"), rule: { target: "messages", position: "last_nth", index: 1, ttl: "5m" } },
  ];

  const fillFirstEmptySlot = (rule: CacheBreakpointRule) => {
    const emptyIdx = slots.findIndex((s) => s === null);
    if (emptyIdx >= 0) {
      updateSlot(emptyIdx, rule);
    }
  };

  return (
    <CollapsibleSection
      title={t("providers.cacheBreakpoints.title")}
      summary={t("providers.cacheBreakpoints.summary", { count: rules.length })}
      expanded={expanded}
      onToggle={() => setExpanded((v) => !v)}
      expandLabel={t("common.show")}
      collapseLabel={t("providers.dispatch.collapse")}
      actions={
        <Button variant="neutral" onClick={() => commit(RECOMMENDED_CACHE_TEMPLATE)}>
          {t("providers.cacheBreakpoints.template")}
        </Button>
      }
    >
      <p className="text-xs text-muted">{t("providers.cacheBreakpoints.hint")}</p>

      {/* Example cards — click to fill first empty slot */}
      <div className="mb-3">
        <div className="mb-1.5 text-xs text-muted">{t("providers.cacheBreakpoints.examples")}</div>
        <div className="grid grid-cols-2 gap-2 xl:grid-cols-4">
          {exampleCards.map((card, i) => (
            <button
              key={i}
              type="button"
              className="rounded-lg border border-dashed border-border px-2 py-2.5 text-center text-xs font-medium text-muted transition hover:border-text hover:text-text"
              onClick={() => fillFirstEmptySlot(card.rule)}
            >
              {card.label}
            </button>
          ))}
        </div>
      </div>
      <div className="grid gap-3 sm:grid-cols-2">
        {slots.map((rule, idx) => (
          <div key={idx} className="rounded-xl border border-border bg-panel-muted px-3 py-2.5">
            <div className="mb-2 flex items-center justify-between">
              <span className="text-xs font-semibold uppercase tracking-[0.08em] text-muted">
                {t("providers.cacheBreakpoints.slot", { index: idx + 1 })}
              </span>
              {rule ? (
                <button
                  type="button"
                  className="text-xs text-muted hover:text-text"
                  onClick={() => clearSlot(idx)}
                >
                  {t("common.delete")}
                </button>
              ) : null}
            </div>
            {rule ? (
              <div className="space-y-2">
                {/* Target: segmented buttons */}
                <div className="flex flex-wrap gap-1">
                  {([
                    { value: "top_level" as const, label: t("providers.cacheBreakpoints.target.topLevel") },
                    { value: "tools" as const, label: t("providers.cacheBreakpoints.target.tools") },
                    { value: "system" as const, label: t("providers.cacheBreakpoints.target.system") },
                    { value: "messages" as const, label: t("providers.cacheBreakpoints.target.messages") },
                  ]).map((item) => (
                    <button
                      key={item.value}
                      type="button"
                      className={`btn rounded-full px-2.5 py-1 text-[11px] font-semibold transition ${
                        rule.target === item.value ? "btn-primary" : "btn-neutral"
                      }`}
                      onClick={() =>
                        updateSlot(idx, {
                          target: item.value,
                          ...(item.value === "top_level"
                            ? { position: "nth" as const, index: 1, content_position: undefined, content_index: undefined }
                            : {}),
                          ...(item.value !== "messages"
                            ? { content_position: undefined, content_index: undefined }
                            : {}),
                        })
                      }
                    >
                      {item.label}
                    </button>
                  ))}
                </div>

                {/* Position + index (non-top_level) */}
                {rule.target !== "top_level" ? (
                  <div className="flex items-center gap-1">
                    <button
                      type="button"
                      className={`btn rounded-full px-2 py-0.5 text-[11px] font-semibold transition ${
                        rule.position === "nth" ? "btn-primary" : "btn-neutral"
                      }`}
                      onClick={() => updateSlot(idx, { position: rule.position === "nth" ? "last_nth" : "nth" })}
                    >
                      {rule.position === "last_nth" ? t("providers.cacheBreakpoints.lastNth") : t("providers.cacheBreakpoints.nth")}
                    </button>
                    <Input
                      value={String(rule.index)}
                      onChange={(v) =>
                        updateSlot(idx, { index: Math.max(1, Number.parseInt(v, 10) || 1) })
                      }
                    />
                    <span className="text-[11px] text-muted">{t("providers.cacheBreakpoints.nthSuffix")}</span>
                  </div>
                ) : null}

                {/* TTL: segmented buttons */}
                <div className="flex gap-1">
                  {(["auto", "5m", "1h"] as const).map((ttl) => (
                    <button
                      key={ttl}
                      type="button"
                      className={`btn rounded-full px-2.5 py-1 text-[11px] font-semibold transition ${
                        rule.ttl === ttl ? "btn-primary" : "btn-neutral"
                      }`}
                      onClick={() => updateSlot(idx, { ttl })}
                    >
                      {ttl === "auto" ? "auto" : ttl}
                    </button>
                  ))}
                </div>
              </div>
            ) : (
              <button
                type="button"
                className="flex w-full items-center justify-center rounded-lg border border-dashed border-border py-8 text-sm text-muted transition hover:border-text hover:text-text"
                onClick={() =>
                  updateSlot(idx, { target: "messages", position: "last_nth", index: 1, ttl: "auto" })
                }
              >
                +
              </button>
            )}
          </div>
        ))}
      </div>
    </CollapsibleSection>
  );
}
