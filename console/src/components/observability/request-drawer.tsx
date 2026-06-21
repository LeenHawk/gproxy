import { useQuery } from "@tanstack/react-query";
import { ChevronsUpDown } from "lucide-react";
import { useTranslation } from "react-i18next";
import { downstreamLogsQuery, upstreamLogsQuery } from "@/api/usage";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible";
import {
  Sheet,
  SheetContent,
  SheetHeader,
  SheetTitle,
} from "@/components/ui/sheet";
import { Skeleton } from "@/components/ui/skeleton";

interface RequestDrawerProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  requestId: string | null;
}

function statusVariant(status: number): "default" | "secondary" | "destructive" | "outline" {
  if (status >= 500) return "destructive";
  if (status >= 400) return "outline";
  if (status >= 200 && status < 300) return "secondary";
  return "outline";
}

function JsonCollapsible({ label, data }: { label: string; data: unknown }) {
  if (data == null) return null;
  const text = typeof data === "string" ? data : JSON.stringify(data, null, 2);
  return (
    <Collapsible>
      <CollapsibleTrigger asChild>
        <Button variant="ghost" size="sm" className="h-7 gap-1 text-xs text-muted-foreground px-2">
          <ChevronsUpDown className="size-3" aria-hidden />
          {label}
        </Button>
      </CollapsibleTrigger>
      <CollapsibleContent>
        <pre className="mt-1 max-h-48 overflow-auto rounded-md bg-muted p-2 text-xs font-mono">
          {text}
        </pre>
      </CollapsibleContent>
    </Collapsible>
  );
}

export function RequestDrawer({ open, onOpenChange, requestId }: RequestDrawerProps) {
  const { t } = useTranslation("observability");
  const enabled = open && !!requestId;
  const rid = requestId ?? "";

  const { data: downstream, isPending: downPending, isError: downError } =
    useQuery({ ...downstreamLogsQuery(rid), enabled });
  const { data: upstream, isPending: upPending, isError: upError } =
    useQuery({ ...upstreamLogsQuery(rid), enabled });

  const pending = downPending || upPending;
  const errored = !pending && (downError || upError);
  const bothEmpty =
    !pending && !errored && (downstream?.length ?? 0) === 0 && (upstream?.length ?? 0) === 0;

  return (
    <Sheet open={open} onOpenChange={onOpenChange}>
      <SheetContent side="right" className="w-full max-w-lg overflow-y-auto p-0 sm:max-w-xl">
        <SheetHeader className="border-b p-4">
          <SheetTitle className="font-mono text-sm">{t("logs.title")}</SheetTitle>
          {requestId && (
            <p className="truncate font-mono text-xs text-muted-foreground">{requestId}</p>
          )}
        </SheetHeader>

        <div className="space-y-6 p-4">
          {pending && (
            <div aria-busy="true" className="space-y-3">
              <Skeleton className="h-6 w-40" />
              <Skeleton className="h-24" />
              <Skeleton className="h-6 w-40" />
              <Skeleton className="h-32" />
            </div>
          )}

          {errored && (
            <p className="py-6 text-center text-sm text-destructive">
              {t("logs.empty")}
            </p>
          )}

          {!pending && bothEmpty && (
            <p className="py-6 text-center text-sm text-muted-foreground">
              {t("logs.notCaptured")}
            </p>
          )}

          {/* Downstream request */}
          {!pending && (downstream?.length ?? 0) > 0 && (
            <section>
              <h3 className="mb-3 text-xs font-semibold uppercase tracking-wider text-muted-foreground">
                {t("logs.downstream")}
              </h3>
              {downstream!.map((req) => (
                <div key={req.id} className="rounded-md border p-3 space-y-2">
                  <div className="flex items-center gap-2 flex-wrap">
                    <Badge variant="outline" className="font-mono text-xs">{req.method}</Badge>
                    <span className="flex-1 truncate font-mono text-xs">{req.path}{req.query ? `?${req.query}` : ""}</span>
                    <Badge variant={statusVariant(req.status)}>{req.status}</Badge>
                  </div>
                  <JsonCollapsible label={t("logs.headers")} data={req.headers_json} />
                  {req.body != null && (
                    <JsonCollapsible label={t("logs.body")} data={req.body} />
                  )}
                  {req.response_body != null && (
                    <JsonCollapsible label={t("logs.responseBody")} data={req.response_body} />
                  )}
                </div>
              ))}
            </section>
          )}

          {/* Upstream requests (may be multiple — one per failover attempt) */}
          {!pending && (upstream?.length ?? 0) > 0 && (
            <section>
              <h3 className="mb-3 text-xs font-semibold uppercase tracking-wider text-muted-foreground">
                {t("logs.upstream")}
              </h3>
              <div className="space-y-3">
                {upstream!.map((req, i) => (
                  <div key={req.id} className="rounded-md border p-3 space-y-2">
                    {upstream!.length > 1 && (
                      <p className="text-xs text-muted-foreground">
                        {t("logs.attempt", { defaultValue: "Attempt {{n}}", n: i + 1 })}
                      </p>
                    )}
                    <div className="flex items-center gap-2 flex-wrap">
                      <Badge variant="outline" className="font-mono text-xs">{req.method}</Badge>
                      <span className="flex-1 truncate font-mono text-xs text-muted-foreground">{req.url}</span>
                      <Badge variant={statusVariant(req.status)}>{req.status}</Badge>
                      <span className="text-xs text-muted-foreground">{req.latency_ms}ms</span>
                    </div>
                    <JsonCollapsible label={t("logs.headers")} data={req.headers_json} />
                    {req.body != null && (
                      <JsonCollapsible label={t("logs.body")} data={req.body} />
                    )}
                    {req.response_body != null && (
                      <JsonCollapsible label={t("logs.responseBody")} data={req.response_body} />
                    )}
                  </div>
                ))}
              </div>
            </section>
          )}
        </div>
      </SheetContent>
    </Sheet>
  );
}
