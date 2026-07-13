import { useState } from "react";
import { useMutation } from "@tanstack/react-query";
import {
  CheckCircle2,
  CircleGauge,
  Cloud,
  Globe2,
  LoaderCircle,
  MapPin,
  Wifi,
  XCircle,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import {
  testConnectivity,
  type ConnectivityScope,
  type ConnectivityTestResult,
} from "@/api/connectivity";
import { ApiError } from "@/api/http";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";

interface ProxyConnectivityTestProps {
  scope: ConnectivityScope;
  proxyUrl: string;
  providerId?: number;
}

function Metric({ icon, label, value }: { icon: React.ReactNode; label: string; value: string }) {
  return (
    <div className="rounded-lg bg-background/70 p-2.5 shadow-sm ring-1 ring-border/60">
      <div className="flex items-center gap-1.5 text-[11px] text-muted-foreground">
        {icon}
        {label}
      </div>
      <div className="mt-1 truncate text-sm font-semibold" title={value}>{value}</div>
    </div>
  );
}

export function ProxyConnectivityTest({ scope, proxyUrl, providerId }: ProxyConnectivityTestProps) {
  const { t } = useTranslation("common");
  const [testedSignature, setTestedSignature] = useState<string | null>(null);
  const signature = `${scope}:${providerId ?? ""}:${proxyUrl.trim()}`;
  const mutation = useMutation({
    mutationFn: () => testConnectivity({
      scope,
      proxy_url: proxyUrl.trim() || null,
      ...(providerId != null ? { provider_id: providerId } : {}),
    }),
    onSuccess: () => setTestedSignature(signature),
    onError: () => setTestedSignature(signature),
  });
  const visible = testedSignature === signature;
  const result = visible ? mutation.data : undefined;
  const requestError = visible && mutation.error
    ? (mutation.error instanceof ApiError ? mutation.error.message : String(mutation.error))
    : null;

  return (
    <div className="grid gap-2" aria-live="polite">
      <Button
        type="button"
        variant="outline"
        className="w-fit"
        disabled={mutation.isPending}
        onClick={() => mutation.mutate()}
      >
        {mutation.isPending
          ? <LoaderCircle className="size-4 animate-spin" aria-hidden />
          : <Wifi className="size-4" aria-hidden />}
        {mutation.isPending ? t("proxyTest.testing") : t("proxyTest.action")}
      </Button>

      {result?.ok && <SuccessResult result={result} />}
      {(result && !result.ok) && (
        <FailureResult
          message={t(`proxyTest.errors.${result.error_code ?? "unknown"}`, {
            defaultValue: result.message ?? t("proxyTest.errors.unknown"),
          })}
          latency={result.latency_ms}
        />
      )}
      {requestError && <FailureResult message={requestError} />}
    </div>
  );
}

function SuccessResult({ result }: { result: ConnectivityTestResult }) {
  const { t } = useTranslation("common");
  const place = [result.colo, result.location].filter(Boolean).join(" · ") || "—";
  return (
    <div className="overflow-hidden rounded-xl border border-emerald-500/35 bg-gradient-to-br from-emerald-500/12 via-emerald-500/5 to-cyan-500/10">
      <div className="flex items-start justify-between gap-3 p-3.5">
        <div className="flex min-w-0 items-start gap-3">
          <div className="rounded-full bg-emerald-500/15 p-2 text-emerald-600 dark:text-emerald-400">
            <CheckCircle2 className="size-5" aria-hidden />
          </div>
          <div className="min-w-0">
            <div className="font-semibold text-emerald-800 dark:text-emerald-300">{t("proxyTest.success")}</div>
            <div className="text-xs text-muted-foreground">{t("proxyTest.successHint")}</div>
          </div>
        </div>
        <Badge className="shrink-0 border-emerald-500/25 bg-emerald-500/10 text-emerald-700 hover:bg-emerald-500/10 dark:text-emerald-300">
          IPv{result.ip_version}
        </Badge>
      </div>

      <div className="border-y border-emerald-500/15 bg-background/35 px-4 py-3">
        <div className="text-[11px] uppercase tracking-wider text-muted-foreground">{t("proxyTest.egressIp")}</div>
        <div className="mt-0.5 break-all font-mono text-lg font-semibold tracking-tight">{result.ip}</div>
      </div>

      <div className="grid grid-cols-3 gap-2 p-3">
        <Metric icon={<MapPin className="size-3" />} label={t("proxyTest.edge")} value={place} />
        <Metric icon={<CircleGauge className="size-3" />} label={t("proxyTest.latency")} value={`${result.latency_ms} ms`} />
        <Metric icon={<Globe2 className="size-3" />} label={t("proxyTest.route")} value={t(`proxyTest.sources.${result.proxy_source}`)} />
      </div>
      <div className="flex items-center gap-1.5 px-4 pb-3 text-[11px] text-muted-foreground">
        <Cloud className="size-3" aria-hidden />
        {t("proxyTest.poweredBy")}
      </div>
    </div>
  );
}

function FailureResult({ message, latency }: { message: string; latency?: number }) {
  const { t } = useTranslation("common");
  return (
    <div className="flex items-start gap-3 rounded-xl border border-destructive/30 bg-destructive/5 p-3.5">
      <div className="rounded-full bg-destructive/10 p-2 text-destructive">
        <XCircle className="size-5" aria-hidden />
      </div>
      <div className="min-w-0">
        <div className="font-semibold text-destructive">{t("proxyTest.failure")}</div>
        <div className="break-words text-xs text-muted-foreground">{message}</div>
        {latency != null && <div className="mt-1 text-[11px] text-muted-foreground">{latency} ms</div>}
      </div>
    </div>
  );
}
