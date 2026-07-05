import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Loader2, TriangleAlert } from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { ApiError } from "@/api/http";
import {
  applyUpdate,
  updateCheckQuery,
  updateStatusQuery,
  type UpdateSafetyRisk,
  type UpdateStatus,
} from "@/api/update";
import { ConfirmDangerous } from "@/components/confirm-dangerous";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";

/** Self-update controls (check / status / apply). Rendered inside the Settings
 *  page's "Updates" tab. */
export function UpdatePanel() {
  const { t } = useTranslation("update");
  const qc = useQueryClient();

  const check = useQuery(updateCheckQuery);
  const checkData = check.data;
  const checkError = check.error as ApiError | null;
  const safety = checkData?.safety ?? [];
  const safetyText = formatSafetyRisks(safety, t);

  const [confirmOpen, setConfirmOpen] = useState(false);
  const [securityConfirmOpen, setSecurityConfirmOpen] = useState(false);
  const [securityWarning, setSecurityWarning] = useState("");
  const apply = useMutation({
    mutationFn: (allowInsecure: boolean) => applyUpdate({ allow_insecure: allowInsecure }),
    onSuccess: () => {
      setConfirmOpen(false);
      setSecurityConfirmOpen(false);
      setSecurityWarning("");
      void qc.invalidateQueries({ queryKey: ["update", "status"] });
      toast.success(t("status.restarting"));
    },
    onError: (e) => {
      if (e instanceof ApiError && e.type === "confirmation_required") {
        setConfirmOpen(false);
        setSecurityWarning(e.message);
        setSecurityConfirmOpen(true);
        return;
      }
      setConfirmOpen(false);
      setSecurityConfirmOpen(false);
      toast.error(e instanceof ApiError ? e.message : String(e));
    },
  });

  const applying = apply.isPending;
  const status = useQuery({
    ...updateStatusQuery,
    refetchInterval: applying ? 2000 : false,
  });
  const statusData = status.data;

  return (
    <div className="grid gap-4">
      <div className="grid gap-4 md:grid-cols-2">
        {/* Check section */}
        <Card>
          <CardHeader>
            <CardTitle className="text-base">{t("check.button")}</CardTitle>
          </CardHeader>
          <CardContent className="grid gap-3">
            <Button
              variant="outline"
              disabled={check.isFetching}
              onClick={() => { void check.refetch(); }}
              aria-busy={check.isFetching}
            >
              {check.isFetching && <Loader2 className="mr-2 size-4 animate-spin" aria-hidden />}
              {t("check.button")}
            </Button>

            {check.isError && (
              <div role="alert" className="rounded-md border border-destructive bg-destructive/10 p-3 text-sm text-destructive">
                {`${t("check.genericError")}: ${checkError?.message ?? ""}`}
              </div>
            )}

            {checkData && (
              <dl className="grid gap-1 text-sm">
                <div className="flex justify-between gap-2">
                  <dt className="text-muted-foreground">{t("check.current")}</dt>
                  <dd className="font-mono">{checkData.current}</dd>
                </div>
                <div className="flex justify-between gap-2">
                  <dt className="text-muted-foreground">{t("check.latest")}</dt>
                  <dd className="font-mono">{checkData.latest}</dd>
                </div>
                <div className="flex justify-between gap-2 pt-1">
                  <dt className="text-muted-foreground">{t("check.available")}</dt>
                  <dd>
                    <Badge variant={checkData.available ? "default" : "secondary"}>
                      {checkData.available ? t("check.availableYes") : t("check.availableNo")}
                    </Badge>
                  </dd>
                </div>
                {checkData.notes_url && (
                  <div className="pt-1">
                    <a
                      href={checkData.notes_url}
                      target="_blank"
                      rel="noopener noreferrer"
                      className="text-sm text-primary underline-offset-4 hover:underline"
                    >
                      {t("check.notes")}
                    </a>
                  </div>
                )}
              </dl>
            )}

            {safety.length > 0 && (
              <div role="alert" className="flex gap-2 rounded-md border border-amber-500 bg-amber-50 p-3 text-sm text-amber-900 dark:bg-amber-950 dark:text-amber-200">
                <TriangleAlert className="mt-0.5 size-4 shrink-0" aria-hidden />
                <span>{t("check.safetyWarning", { risks: safetyText })}</span>
              </div>
            )}
          </CardContent>
        </Card>

        {/* Status section */}
        <Card>
          <CardHeader>
            <CardTitle className="text-base">{t("status.label")}</CardTitle>
          </CardHeader>
          <CardContent>
            <div role="status" aria-live="polite" aria-busy={status.isFetching || applying}>
              <StatusDisplay statusData={statusData} t={t} />
            </div>
          </CardContent>
        </Card>
      </div>

      {/* Apply section — shown only when check returned available===true */}
      {checkData?.available && (
        <div>
          <Button variant="default" disabled={applying} onClick={() => setConfirmOpen(true)}>
            {applying && <Loader2 className="mr-2 size-4 animate-spin" aria-hidden />}
            {t("apply.button")}
          </Button>
        </div>
      )}

      <ConfirmDangerous
        open={confirmOpen}
        onOpenChange={setConfirmOpen}
        title={t("apply.confirm.title")}
        description={t("apply.confirm.description")}
        confirmLabel={t("apply.confirm.confirmLabel")}
        onConfirm={() => apply.mutate(false)}
        pending={applying}
      />

      <ConfirmDangerous
        open={securityConfirmOpen}
        onOpenChange={setSecurityConfirmOpen}
        title={t("apply.securityConfirm.title")}
        description={(
          <span className="grid gap-2">
            <span>{t("apply.securityConfirm.description")}</span>
            {securityWarning && <span className="break-words font-mono text-xs">{securityWarning}</span>}
          </span>
        )}
        confirmLabel={t("apply.securityConfirm.confirmLabel")}
        onConfirm={() => apply.mutate(true)}
        pending={applying}
      />
    </div>
  );
}

function formatSafetyRisks(safety: UpdateSafetyRisk[], t: (key: string) => string) {
  return safety.map((risk) => t(`check.safety.${risk}`)).join(", ");
}

function StatusDisplay({
  statusData,
  t,
}: {
  statusData: UpdateStatus | undefined;
  t: (key: string) => string;
}) {
  if (!statusData) {
    return <p className="text-sm text-muted-foreground">—</p>;
  }
  switch (statusData.state) {
    case "idle":
      return <p className="text-sm text-muted-foreground">{t("status.idle")}</p>;
    case "checking":
    case "downloading":
      return (
        <div className="flex items-center gap-2 text-sm text-muted-foreground">
          <Loader2 className="size-4 animate-spin" aria-hidden />
          {statusData.state === "checking" ? t("status.checking") : t("status.downloading")}
        </div>
      );
    case "staged":
      return (
        <div className="grid gap-2">
          <p className="font-medium text-green-700 dark:text-green-400">
            {t("status.staged")} — v{statusData.version}
          </p>
          <p className="text-sm text-muted-foreground">{t("status.stagedRestartNotice")}</p>
        </div>
      );
    case "restarting":
      return (
        <div className="flex items-center gap-2 text-sm text-muted-foreground">
          <Loader2 className="size-4 animate-spin" aria-hidden />
          {t("status.restarting")} — v{statusData.version}
        </div>
      );
    case "failed":
      return (
        <div className="grid gap-1">
          <p className="font-medium text-destructive">{t("status.failed")}</p>
          <p className="text-sm text-destructive/80">{statusData.error}</p>
        </div>
      );
  }
}
