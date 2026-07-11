import { useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { upsertRoute, ROUTE_STRATEGIES, type Route } from "@/api/routes";
import { ApiError } from "@/api/http";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Select, SelectContent, SelectItem, SelectTrigger, SelectValue,
} from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";

interface BreakerState {
  consecutiveFailures: string;
  cooldownSecs: string;
  errorRateEnabled: boolean;
  windowSecs: string;
  thresholdPercent: string;
  minRequests: string;
}

function objectValue(value: unknown): Record<string, unknown> {
  return value !== null && typeof value === "object" && !Array.isArray(value)
    ? value as Record<string, unknown>
    : {};
}

function initialBreaker(settings: unknown): BreakerState {
  const breaker = objectValue(objectValue(settings).circuit_breaker);
  const errorRate = objectValue(breaker.error_rate);
  const threshold = typeof errorRate.threshold === "number"
    ? String(errorRate.threshold * 100)
    : "50";
  return {
    consecutiveFailures: typeof breaker.consecutive_failures === "number" ? String(breaker.consecutive_failures) : "",
    cooldownSecs: typeof breaker.cooldown_secs === "number" ? String(breaker.cooldown_secs) : "",
    errorRateEnabled: Object.keys(errorRate).length > 0,
    windowSecs: typeof errorRate.window_secs === "number" ? String(errorRate.window_secs) : "60",
    thresholdPercent: threshold,
    minRequests: typeof errorRate.min_requests === "number" ? String(errorRate.min_requests) : "20",
  };
}

function positiveInteger(value: string): number | null {
  if (!value.trim()) return null;
  const parsed = Number(value);
  return Number.isInteger(parsed) && parsed > 0 ? parsed : NaN;
}

export function RouteForm({ route, onSaved }: { route?: Route; onSaved: (saved: Route) => void }) {
  const { t } = useTranslation("routes");
  const queryClient = useQueryClient();
  const editing = route !== undefined;

  const [name, setName] = useState(route?.name ?? "");
  const [strategy, setStrategy] = useState(route?.strategy ?? "failover");
  const [description, setDescription] = useState(route?.description ?? "");
  const [enabled, setEnabled] = useState(route?.enabled ?? true);
  const [breaker, setBreaker] = useState<BreakerState>(() => initialBreaker(route?.settings_json));
  const [formError, setFormError] = useState<string | null>(null);

  const mutation = useMutation({
    mutationFn: () => {
      if (!name.trim()) throw new ApiError(0, "bad_request", t("form.required"));
      const consecutiveFailures = positiveInteger(breaker.consecutiveFailures);
      const cooldownSecs = positiveInteger(breaker.cooldownSecs);
      if (Number.isNaN(consecutiveFailures) || Number.isNaN(cooldownSecs)) {
        throw new ApiError(0, "bad_request", t("form.positiveInteger"));
      }

      const settings = { ...objectValue(route?.settings_json) };
      const circuitBreaker = { ...objectValue(settings.circuit_breaker) };
      if (consecutiveFailures === null) delete circuitBreaker.consecutive_failures;
      else circuitBreaker.consecutive_failures = consecutiveFailures;
      if (cooldownSecs === null) delete circuitBreaker.cooldown_secs;
      else circuitBreaker.cooldown_secs = cooldownSecs;

      if (breaker.errorRateEnabled) {
        const windowSecs = positiveInteger(breaker.windowSecs);
        const minRequests = positiveInteger(breaker.minRequests);
        const thresholdPercent = Number(breaker.thresholdPercent);
        if (
          windowSecs === null || Number.isNaN(windowSecs)
          || minRequests === null || Number.isNaN(minRequests)
          || !Number.isFinite(thresholdPercent)
          || thresholdPercent <= 0 || thresholdPercent > 100
        ) {
          throw new ApiError(0, "bad_request", t("form.errorRateInvalid"));
        }
        circuitBreaker.error_rate = {
          window_secs: windowSecs,
          threshold: thresholdPercent / 100,
          min_requests: minRequests,
        };
      } else {
        delete circuitBreaker.error_rate;
      }

      if (Object.keys(circuitBreaker).length > 0) settings.circuit_breaker = circuitBreaker;
      else delete settings.circuit_breaker;
      return upsertRoute({
        id: route?.id ?? null,
        name: name.trim(),
        strategy,
        enabled,
        description: description.trim() === "" ? null : description.trim(),
        ...(Object.keys(settings).length > 0 ? { settings_json: settings } : {}),
      });
    },
    onSuccess: (saved) => {
      void queryClient.invalidateQueries({ queryKey: ["routes"] });
      toast.success(t("form.saved"));
      onSaved(saved);
    },
    onError: (error) => setFormError(error instanceof ApiError ? error.message : String(error)),
  });

  return (
    <form className="grid gap-4" onSubmit={(e) => { e.preventDefault(); setFormError(null); mutation.mutate(); }}>
      <div className="grid gap-2">
        <Label htmlFor="r-name">{t("fields.name")}</Label>
        <Input id="r-name" value={name} onChange={(e) => setName(e.target.value)} required />
      </div>
      <div className="grid gap-2">
        <Label>{t("fields.strategy")}</Label>
        <Select value={strategy} onValueChange={setStrategy}>
          <SelectTrigger><SelectValue /></SelectTrigger>
          <SelectContent>
            {ROUTE_STRATEGIES.map((s) => (
              <SelectItem key={s} value={s}>{t(`strategy.${s}`)}</SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>
      <div className="grid gap-2">
        <Label htmlFor="r-desc">{t("fields.description")}</Label>
        <Input id="r-desc" value={description} onChange={(e) => setDescription(e.target.value)} />
      </div>
      <div className="grid gap-3 rounded-md border p-3">
        <div>
          <Label>{t("breaker.title")}</Label>
          <p className="mt-1 text-xs text-muted-foreground">{t("breaker.inheritHint")}</p>
        </div>
        <div className="grid grid-cols-2 gap-3">
          <div className="grid gap-1">
            <Label htmlFor="r-consecutive-failures" className="text-xs font-normal text-muted-foreground">
              {t("breaker.consecutiveFailures")}
            </Label>
            <Input
              id="r-consecutive-failures"
              type="number"
              min="1"
              value={breaker.consecutiveFailures}
              onChange={(event) => setBreaker((current) => ({ ...current, consecutiveFailures: event.target.value }))}
              placeholder={t("breaker.inherit")}
            />
          </div>
          <div className="grid gap-1">
            <Label htmlFor="r-cooldown-secs" className="text-xs font-normal text-muted-foreground">
              {t("breaker.cooldownSecs")}
            </Label>
            <Input
              id="r-cooldown-secs"
              type="number"
              min="1"
              value={breaker.cooldownSecs}
              onChange={(event) => setBreaker((current) => ({ ...current, cooldownSecs: event.target.value }))}
              placeholder={t("breaker.inherit")}
            />
          </div>
        </div>
        <div className="flex items-center justify-between gap-4 border-t pt-3">
          <div>
            <Label htmlFor="r-error-rate">{t("breaker.errorRate")}</Label>
            <p className="text-xs text-muted-foreground">{t("breaker.errorRateHint")}</p>
          </div>
          <Switch
            id="r-error-rate"
            checked={breaker.errorRateEnabled}
            onCheckedChange={(checked) => setBreaker((current) => ({ ...current, errorRateEnabled: checked }))}
          />
        </div>
        {breaker.errorRateEnabled && (
          <div className="grid grid-cols-3 gap-3">
            <div className="grid gap-1">
              <Label htmlFor="r-window-secs" className="text-xs font-normal text-muted-foreground">{t("breaker.windowSecs")}</Label>
              <Input id="r-window-secs" type="number" min="1" value={breaker.windowSecs} onChange={(event) => setBreaker((current) => ({ ...current, windowSecs: event.target.value }))} />
            </div>
            <div className="grid gap-1">
              <Label htmlFor="r-threshold" className="text-xs font-normal text-muted-foreground">{t("breaker.threshold")}</Label>
              <Input id="r-threshold" type="number" min="0.01" max="100" step="0.01" value={breaker.thresholdPercent} onChange={(event) => setBreaker((current) => ({ ...current, thresholdPercent: event.target.value }))} />
            </div>
            <div className="grid gap-1">
              <Label htmlFor="r-min-requests" className="text-xs font-normal text-muted-foreground">{t("breaker.minRequests")}</Label>
              <Input id="r-min-requests" type="number" min="1" value={breaker.minRequests} onChange={(event) => setBreaker((current) => ({ ...current, minRequests: event.target.value }))} />
            </div>
          </div>
        )}
      </div>
      <div className="flex items-center justify-between">
        <Label htmlFor="r-enabled">{t("fields.enabled")}</Label>
        <Switch id="r-enabled" checked={enabled} onCheckedChange={setEnabled} />
      </div>
      {formError && <p className="text-sm text-destructive">{formError}</p>}
      <Button type="submit" disabled={mutation.isPending}>
        {editing ? t("form.edit") : t("form.create")}
      </Button>
    </form>
  );
}
