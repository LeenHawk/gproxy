import { useState } from "react";
import { useTranslation } from "react-i18next";
import { OPERATIONS } from "@/api/rules";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Badge } from "@/components/ui/badge";
import { Popover, PopoverAnchor, PopoverContent } from "@/components/ui/popover";
import { cn } from "@/lib/utils";

export function toOperationArray(v: unknown): string[] {
  return Array.isArray(v) ? v.filter((x): x is string => typeof x === "string") : [];
}
export function fromOperationArray(a: string[]): string[] | null {
  return a.length ? a : null;
}

function modelPatternSuggestions(value: string, modelOptions: string[]): string[] {
  const exactSelection = modelOptions.some((model) => model === value);
  const query = exactSelection ? "" : value.toLowerCase();
  return modelOptions
    .filter((model) => model !== value && model.toLowerCase().includes(query))
    .slice(0, 8);
}

export function ModelPatternField({
  value, onChange, modelOptions,
}: { value: string; onChange: (v: string) => void; modelOptions?: string[] }) {
  const { t } = useTranslation("rules");
  const [open, setOpen] = useState(false);
  const matches = modelPatternSuggestions(value, modelOptions ?? []);
  const showPopover = open && (modelOptions?.length ?? 0) > 0 && matches.length > 0;
  return (
    <div className="grid gap-1">
      <Label htmlFor="rule-fmp">{t("filter.modelGlobLabel")}</Label>
      <Popover open={showPopover} onOpenChange={setOpen}>
        <PopoverAnchor asChild>
          <Input
            id="rule-fmp"
            value={value}
            onChange={(e) => { onChange(e.target.value); setOpen(true); }}
            onFocus={() => setOpen(true)}
            autoComplete="off"
            role="combobox"
            aria-autocomplete="list"
            aria-expanded={showPopover}
            placeholder={t("filter.modelGlobPlaceholder")}
          />
        </PopoverAnchor>
        <PopoverContent
          className="w-[--radix-popover-trigger-width] p-1"
          align="start"
          onOpenAutoFocus={(e) => e.preventDefault()}
        >
          {matches.map((m) => (
            <button
              key={m}
              type="button"
              className="block w-full rounded px-2 py-1 text-left text-sm hover:bg-muted"
              onClick={() => { onChange(m); setOpen(false); }}
            >
              {m}
            </button>
          ))}
        </PopoverContent>
      </Popover>
      <p className="text-xs text-muted-foreground">{t("filter.modelGlobHelp")}</p>
    </div>
  );
}

export const CLIENT_HEADER_PRESETS = [
  "^user-agent: opencode/",
  "^user-agent: claude-cli/",
  "^user-agent: codex",
  "(?i)\\bcursor\\b",
] as const;

/// Inbound-header regex: scopes a rule to one client (matched against every
/// `name: value` line of the ORIGINAL client request).
export function ClientHeaderField({
  value, onChange,
}: { value: string; onChange: (v: string) => void }) {
  const { t } = useTranslation("rules");
  return (
    <div className="grid gap-1">
      <Label htmlFor="rule-fhp">{t("filter.clientGlobLabel")}</Label>
      <Input
        id="rule-fhp"
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder={t("filter.clientGlobPlaceholder")}
      />
      <div className="flex flex-wrap gap-1.5">
        {CLIENT_HEADER_PRESETS.map((preset) => (
          <Badge
            key={preset}
            role="button"
            tabIndex={0}
            variant={value === preset ? "secondary" : "outline"}
            className={cn("cursor-pointer select-none", value === preset && "ring-1 ring-primary")}
            onClick={() => onChange(value === preset ? "" : preset)}
            onKeyDown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); onChange(value === preset ? "" : preset); } }}
          >
            {preset}
          </Badge>
        ))}
      </div>
      <p className="text-xs text-muted-foreground">{t("filter.clientGlobHelp")}</p>
    </div>
  );
}

export function OperationChips({ value, onChange }: { value: string[]; onChange: (v: string[]) => void }) {  const { t } = useTranslation("rules");
  const toggle = (op: string) =>
    onChange(value.includes(op) ? value.filter((x) => x !== op) : [...value, op]);
  return (
    <div className="grid gap-1">
      <Label>{t("filter.operationsLabel")}</Label>
      <div className="flex flex-wrap gap-1.5">
        {OPERATIONS.map((op) => {
          const on = value.includes(op);
          return (
            <Badge
              key={op}
              role="button"
              tabIndex={0}
              variant={on ? "secondary" : "outline"}
              className={cn("cursor-pointer select-none", on && "ring-1 ring-primary")}
              onClick={() => toggle(op)}
              onKeyDown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); toggle(op); } }}
            >
              {t(`operation.${op}`)}
            </Badge>
          );
        })}
      </div>
      <p className="text-xs text-muted-foreground">{t("filter.operationsHelp")}</p>
    </div>
  );
}
