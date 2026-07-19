import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  Select, SelectContent, SelectItem, SelectTrigger, SelectValue,
} from "@/components/ui/select";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  SUFFIX_PROTOCOL_LABELS, UPSTREAM_SOURCE_GROUP_BY_CHANNEL,
  suffixGroupsForChannel, suffixProtocolForChannel,
  type SuffixAction, type SuffixProtocol,
} from "@/components/providers/suffix-presets";

// Radix <SelectItem value=""> crashes at runtime — use a sentinel for "none".
const NONE = "__none__";

function parseUpstreams(value: string): string[] {
  return [...new Set(value.split(",").map((v) => v.trim()).filter(Boolean))];
}

function upstreamSuffix(upstreams: string[]): string {
  const value = upstreams
    .map((v) => v.replace(/[^a-zA-Z0-9._-]+/g, "-").replace(/^-+|-+$/g, ""))
    .filter(Boolean)
    .join("-");
  return value ? `-via-${value}` : "";
}

export function VariantPresetPicker({
  modelId, channel, onConfirm, onCancel,
}: {
  modelId: string;
  channel: string;
  onConfirm: (actions: SuffixAction[], suggestedSuffix: string) => void;
  onCancel: () => void;
}) {
  const { t } = useTranslation("providers");
  const [protocol, setProtocol] = useState<SuffixProtocol>(() => suffixProtocolForChannel(channel));
  // group key → selected entry index (as string), or NONE.
  const [picks, setPicks] = useState<Record<string, string>>({});
  const [upstream, setUpstream] = useState("");

  const groups = suffixGroupsForChannel(protocol, channel);
  // Dropdown and custom input are mutually exclusive — both inject `only`.
  const sourceGroupKey = UPSTREAM_SOURCE_GROUP_BY_CHANNEL[channel]?.key ?? null;
  const upstreamPath = channel === "openrouter"
    ? "provider.only"
    : channel === "vercel"
      ? "providerOptions.gateway.only"
      : null;

  const { suffix, actions } = useMemo(() => {
    let s = "";
    const a: SuffixAction[] = [];
    for (const g of groups) {
      const picked = picks[g.key];
      if (!picked || picked === NONE) continue;
      const entry = g.entries[Number(picked)];
      if (!entry) continue;
      s += entry.suffix;
      a.push(...entry.actions);
    }
    const upstreams = parseUpstreams(upstream);
    if (upstreamPath && upstreams.length > 0) {
      s += upstreamSuffix(upstreams);
      a.push({ path: upstreamPath, value: upstreams });
    }
    return { suffix: s, actions: a };
  }, [groups, picks, upstream, upstreamPath]);

  return (
    <div className="grid gap-3 rounded-md border bg-muted/30 p-3">
      <div className="grid gap-1">
        <Label>{t("models.variantPicker.protocol")}</Label>
        <Select value={protocol} onValueChange={(v) => { setProtocol(v as SuffixProtocol); setPicks({}); }}>
          <SelectTrigger><SelectValue /></SelectTrigger>
          <SelectContent>
            {(Object.keys(SUFFIX_PROTOCOL_LABELS) as SuffixProtocol[]).map((p) => (
              <SelectItem key={p} value={p}>{SUFFIX_PROTOCOL_LABELS[p]}</SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>

      {groups.map((g) => (
        <div key={g.key} className="grid gap-1">
          <Label>{g.label}</Label>
          <Select
            value={picks[g.key] ?? NONE}
            onValueChange={(v) => {
              setPicks((prev) => ({ ...prev, [g.key]: v }));
              if (g.key === sourceGroupKey && v !== NONE) setUpstream("");
            }}
          >
            <SelectTrigger><SelectValue /></SelectTrigger>
            <SelectContent>
              <SelectItem value={NONE}>{t("models.variantPicker.none")}</SelectItem>
              {g.entries.map((e, i) => (
                <SelectItem key={e.suffix + i} value={String(i)}>{`${e.suffix} — ${e.label}`}</SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>
      ))}

      {upstreamPath && (
        <div className="grid gap-1">
          <Label htmlFor="variant-upstream">{t("models.variantPicker.upstream")}</Label>
          <Input
            id="variant-upstream"
            className="font-mono text-xs"
            value={upstream}
            placeholder={channel === "openrouter" ? "anthropic, google-vertex/us-east5" : "anthropic, bedrock"}
            onChange={(e) => {
              const value = e.target.value;
              setUpstream(value);
              if (value.trim() && sourceGroupKey) {
                setPicks((prev) => ({ ...prev, [sourceGroupKey]: NONE }));
              }
            }}
          />
          <p className="text-xs text-muted-foreground">
            {t("models.variantPicker.upstreamHint")}
          </p>
        </div>
      )}

      <div className="rounded border bg-background p-2 text-xs">
        <div className="text-muted-foreground">{t("models.variantPicker.suggestedName")}</div>
        <div className="font-mono">{suffix ? `${modelId}${suffix}` : (modelId || "—")}</div>
        {actions.length > 0 && (
          <div className="mt-2 grid gap-1">
            <div className="text-muted-foreground">{t("models.variantPicker.injects")}</div>
            {actions.map((act, i) => (
              <div key={i} className="font-mono">
                <span className="text-foreground">{act.path}</span> = {JSON.stringify(act.value)}
              </div>
            ))}
          </div>
        )}
      </div>

      <div className="flex justify-end gap-2">
        <Button type="button" variant="ghost" size="sm" onClick={onCancel}>
          {t("models.variantPicker.cancel")}
        </Button>
        <Button type="button" size="sm" disabled={actions.length === 0} onClick={() => onConfirm(actions, suffix)}>
          {t("models.variantPicker.apply")}
        </Button>
      </div>
    </div>
  );
}
