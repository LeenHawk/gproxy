import { useId } from "react";
import { CalendarRange } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover";
import {
  fromLocalInput,
  quickFills,
  toLocalInput,
  type TimeRange,
} from "@/lib/time-range";

function fmtEndpoint(unixSecs: number): string {
  return new Date(unixSecs * 1000).toLocaleString(undefined, {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

interface TimeRangePickerProps {
  value: TimeRange;
  onChange: (next: TimeRange) => void;
  /** Charts need both endpoints — hides "clear" and ignores emptied inputs. */
  required?: boolean;
  align?: "start" | "center" | "end";
}

/**
 * Explicit start/end time selection, replacing the rolling `1h`/`24h`/`7d`
 * presets. Quick fills only seed the two endpoints; the endpoints themselves
 * are the single source of truth and stay editable afterwards.
 */
export function TimeRangePicker({
  value,
  onChange,
  required = false,
  align = "start",
}: TimeRangePickerProps) {
  const { t } = useTranslation("common");
  const id = useId();

  const { from, to } = value;
  const invalid = from != null && to != null && from > to;

  function label(): string {
    if (from != null && to != null) {
      return `${fmtEndpoint(from)} → ${fmtEndpoint(to)}`;
    }
    if (from != null) return t("timeRange.after", { time: fmtEndpoint(from) });
    if (to != null) return t("timeRange.before", { time: fmtEndpoint(to) });
    return t("timeRange.anyTime");
  }

  function setEndpoint(key: keyof TimeRange, raw: string) {
    const next = fromLocalInput(raw);
    // In required mode an emptied field would break the rollup query, so the
    // previous endpoint stands until a valid replacement is picked.
    if (next === undefined && required) return;
    onChange({ ...value, [key]: next });
  }

  return (
    <Popover>
      <PopoverTrigger asChild>
        <Button
          variant="outline"
          size="sm"
          aria-label={t("timeRange.label")}
          className="gap-1.5 font-normal"
        >
          <CalendarRange className="size-3.5" aria-hidden />
          {label()}
        </Button>
      </PopoverTrigger>
      <PopoverContent align={align} className="w-72 space-y-3">
        <div className="flex flex-wrap gap-1">
          {quickFills().map(({ key, range }) => (
            <button
              key={key}
              type="button"
              onClick={() => onChange({ ...value, ...range })}
              className="rounded-md border px-2 py-1 text-xs text-muted-foreground hover:bg-accent hover:text-accent-foreground"
            >
              {t(`timeRange.quick.${key}`)}
            </button>
          ))}
        </div>

        <div className="grid gap-2">
          <div className="grid gap-1">
            <Label htmlFor={`${id}-from`} className="text-xs font-normal text-muted-foreground">
              {t("timeRange.start")}
            </Label>
            <Input
              id={`${id}-from`}
              type="datetime-local"
              value={from != null ? toLocalInput(from) : ""}
              aria-invalid={invalid}
              onChange={(e) => setEndpoint("from", e.target.value)}
            />
          </div>
          <div className="grid gap-1">
            <Label htmlFor={`${id}-to`} className="text-xs font-normal text-muted-foreground">
              {t("timeRange.end")}
            </Label>
            <Input
              id={`${id}-to`}
              type="datetime-local"
              value={to != null ? toLocalInput(to) : ""}
              aria-invalid={invalid}
              onChange={(e) => setEndpoint("to", e.target.value)}
            />
          </div>
        </div>

        {invalid && (
          <p className="text-xs text-destructive">{t("timeRange.invalid")}</p>
        )}

        {!required && (
          <Button
            variant="ghost"
            size="sm"
            className="w-full text-muted-foreground"
            onClick={() => onChange({ ...value, from: undefined, to: undefined })}
          >
            {t("timeRange.clear")}
          </Button>
        )}
      </PopoverContent>
    </Popover>
  );
}
