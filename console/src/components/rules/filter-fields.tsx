import { useState } from "react";
import { useTranslation } from "react-i18next";
import { OPERATIONS } from "@/api/rules";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Badge } from "@/components/ui/badge";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { cn } from "@/lib/utils";

export function toOperationArray(v: unknown): string[] {
  return Array.isArray(v) ? v.filter((x): x is string => typeof x === "string") : [];
}
export function fromOperationArray(a: string[]): string[] | null {
  return a.length ? a : null;
}

export function ModelPatternField({
  value, onChange, modelOptions,
}: { value: string; onChange: (v: string) => void; modelOptions?: string[] }) {
  const { t } = useTranslation("rules");
  const [open, setOpen] = useState(false);
  const matches = (modelOptions ?? []).filter(
    (m) => m.toLowerCase().includes(value.toLowerCase()) && m !== value,
  ).slice(0, 8);
  const showPopover = open && (modelOptions?.length ?? 0) > 0 && matches.length > 0;
  return (
    <div className="grid gap-1">
      <Label htmlFor="rule-fmp">{t("filter.modelGlobLabel")}</Label>
      <Popover open={showPopover} onOpenChange={setOpen}>
        <PopoverTrigger asChild>
          <Input
            id="rule-fmp"
            value={value}
            onChange={(e) => { onChange(e.target.value); setOpen(true); }}
            onFocus={() => setOpen(true)}
            placeholder={t("filter.modelGlobPlaceholder")}
          />
        </PopoverTrigger>
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

export function OperationChips({ value, onChange }: { value: string[]; onChange: (v: string[]) => void }) {
  const { t } = useTranslation("rules");
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
