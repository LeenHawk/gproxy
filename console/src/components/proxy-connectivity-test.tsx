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
  type ConnectivityProbeResult,
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
  const probes: Array<[4 | 6, ConnectivityProbeResult]> = [];
  if (result.ipv4) probes.push([4, result.ipv4]);
  if (result.ipv6) probes.push([6, result.ipv6]);

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
        <div className="flex shrink-0 gap-1.5">
          {probes.map(([version]) => (
            <Badge key={version} className="border-emerald-500/25 bg-emerald-500/10 text-emerald-700 hover:bg-emerald-500/10 dark:text-emerald-300">
              IPv{version}
            </Badge>
          ))}
        </div>
      </div>

      <div className="grid gap-2 border-y border-emerald-500/15 bg-background/35 p-3 sm:grid-cols-2">
        {probes.map(([version, probe]) => (
          <ProbeCard key={version} version={version} probe={probe} />
        ))}
      </div>

      <div className="flex flex-wrap items-center justify-between gap-2 px-4 py-3 text-[11px] text-muted-foreground">
        <span className="flex items-center gap-1.5">
          <Globe2 className="size-3" aria-hidden />
          {t("proxyTest.route")}: {t(`proxyTest.sources.${result.proxy_source}`)}
        </span>
        <span className="flex items-center gap-1.5">
          <Cloud className="size-3" aria-hidden />
          {t("proxyTest.poweredBy")}
        </span>
      </div>
    </div>
  );
}

function ProbeCard({ version, probe }: { version: 4 | 6; probe: ConnectivityProbeResult }) {
  const { t } = useTranslation("common");
  const place = [probe.colo, probe.location].filter(Boolean).join(" · ") || "—";
  return (
    <div className="min-w-0 rounded-lg bg-background/75 p-3 shadow-sm ring-1 ring-border/60">
      <div className="flex items-center justify-between gap-2 text-[11px] text-muted-foreground">
        <span className="uppercase tracking-wider">IPv{version} {t("proxyTest.egressIp")}</span>
        <span className="flex items-center gap-1 tabular-nums">
          <CircleGauge className="size-3" aria-hidden />
          {probe.latency_ms} ms
        </span>
      </div>
      <div className="mt-1 break-all font-mono text-base font-semibold tracking-tight">{probe.ip}</div>
      <div className="mt-2 flex items-center gap-1.5 text-xs text-muted-foreground" title={t("proxyTest.edge")}>
        <MapPin className="size-3 shrink-0" aria-hidden />
        <span className="truncate">{place}</span>
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
