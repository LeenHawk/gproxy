import { useI18n } from "../../../app/i18n";
import { Button, Card, Label, Select } from "../../../components/ui";
import { CredentialHealthPanel } from "./CredentialHealthPanel";
import { KpiCards } from "./KpiCards";
import { StatusCodesChart } from "./StatusCodesChart";
import { TopModelsTable } from "./TopModelsTable";
import { TopProvidersTable } from "./TopProvidersTable";
import { TrafficChart } from "./TrafficChart";
import { useDashboardState } from "./useDashboardState";
import type { DashboardPreset } from "./types";

const PRESETS: DashboardPreset[] = ["1h", "24h", "7d", "30d"];

export function DashboardModule({
  sessionToken,
}: {
  sessionToken: string;
  notify: (kind: "success" | "error" | "info", message: string) => void;
}) {
  const { t } = useI18n();
  const state = useDashboardState(sessionToken);

  return (
    <div className="space-y-4">
      <Card title={t("dashboard.title")} subtitle={t("dashboard.subtitle")}>
        <div className="grid gap-4 xl:grid-cols-[minmax(0,1fr)_220px_auto] xl:items-end">
          <div className="space-y-3">
            <div className="flex flex-wrap gap-2">
              {PRESETS.map((preset) => {
                const active = state.range.kind === "preset" && state.range.preset === preset;
                return (
                  <Button
                    key={preset}
                    variant={active ? "primary" : "neutral"}
                    onClick={() => state.selectPreset(preset)}
                  >
                    {t(`dashboard.range.${preset}`)}
                  </Button>
                );
              })}
              <Button
                variant={state.range.kind === "custom" ? "primary" : "neutral"}
                onClick={() => state.applyCustomRange()}
              >
                {t("dashboard.range.custom")}
              </Button>
            </div>
            <div className="grid gap-3 md:grid-cols-3">
              <div>
                <Label>{t("dashboard.custom.from")}</Label>
                <input
                  className="input"
                  type="datetime-local"
                  value={state.customFrom}
                  onChange={(event) => state.setCustomFrom(event.target.value)}
                />
              </div>
              <div>
                <Label>{t("dashboard.custom.to")}</Label>
                <input
                  className="input"
                  type="datetime-local"
                  value={state.customTo}
                  onChange={(event) => state.setCustomTo(event.target.value)}
                />
              </div>
              <div className="flex items-end">
                <Button variant="neutral" onClick={() => state.applyCustomRange()}>
                  {t("dashboard.custom.apply")}
                </Button>
              </div>
            </div>
            {state.customError ? <p className="text-sm text-danger">{state.customError}</p> : null}
          </div>
          <div>
            <Label>{t("dashboard.autoRefresh")}</Label>
            <Select
              value={String(state.autoRefreshMs)}
              onChange={(value) => state.setAutoRefreshMs(Number(value))}
              options={[
                { value: "0", label: t("dashboard.auto.off") },
                { value: "10000", label: t("dashboard.auto.10s") },
                { value: "30000", label: t("dashboard.auto.30s") },
                { value: "60000", label: t("dashboard.auto.60s") },
              ]}
            />
          </div>
          <div className="flex items-end">
            <Button variant="neutral" onClick={() => void state.refresh()}>
              {t("common.refresh")}
            </Button>
          </div>
        </div>
      </Card>

      <KpiCards
        kpi={state.overview.data.kpi}
        loading={state.overview.loading}
        labels={{
          requests: t("dashboard.kpi.requests"),
          successRate: t("dashboard.kpi.successRate"),
          cost: t("dashboard.kpi.cost"),
          tokens: t("dashboard.kpi.tokens"),
          avgLatency: t("dashboard.kpi.avgLatency"),
          maxLatency: t("dashboard.kpi.maxLatency"),
          loading: t("common.loading"),
        }}
      />

      <div className="grid gap-4 xl:grid-cols-2">
        <TrafficChart
          rows={state.overview.data.traffic}
          loading={state.overview.loading}
          error={state.overview.error}
          labels={{
            title: t("dashboard.chart.traffic.title"),
            subtitle: t("dashboard.chart.traffic.subtitle"),
            requests: t("dashboard.kpi.requests"),
            cost: t("dashboard.kpi.cost"),
            noData: t("common.noData"),
          }}
        />
        <StatusCodesChart
          rows={state.overview.data.status_codes}
          loading={state.overview.loading}
          error={state.overview.error}
          labels={{
            title: t("dashboard.chart.status.title"),
            subtitle: t("dashboard.chart.status.subtitle"),
            ok: "2xx",
            err4xx: "4xx",
            err5xx: "5xx",
            noData: t("common.noData"),
          }}
        />
      </div>

      <div className="grid gap-4 xl:grid-cols-2">
        <TopProvidersTable
          rows={state.topProviders.data.rows}
          error={state.topProviders.error}
          labels={{
            title: t("dashboard.table.topProviders"),
            provider: t("common.provider"),
            requests: t("dashboard.table.requests"),
            cost: t("dashboard.kpi.cost"),
            inputTokens: t("common.inputTokens"),
            outputTokens: t("common.outputTokens"),
          }}
        />
        <TopModelsTable
          rows={state.topModels.data.rows}
          error={state.topModels.error}
          labels={{
            title: t("dashboard.table.topModels"),
            model: t("dashboard.table.model"),
            requests: t("dashboard.table.requests"),
            cost: t("dashboard.kpi.cost"),
            inputTokens: t("common.inputTokens"),
            outputTokens: t("common.outputTokens"),
          }}
        />
      </div>

      <CredentialHealthPanel
        rows={state.credentialHealth.data}
        error={state.credentialHealth.error}
        labels={{
          title: t("dashboard.table.credentialHealth"),
          healthy: t("dashboard.health.healthy"),
          cooldown: t("dashboard.health.cooldown"),
          dead: t("dashboard.health.dead"),
        }}
      />
    </div>
  );
}
