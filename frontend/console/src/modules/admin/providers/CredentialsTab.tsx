import { useState } from "react";

import { Badge, Button, Card, Input, Label, TextArea } from "../../../components/ui";
import type { CredentialHealthRow, CredentialRow } from "../../../lib/types/admin";
import { credentialFieldsForChannel } from "./channel-forms";
import { getCredentialUsageActionLabels } from "./credential-usage";
import { summarizeCredential } from "./credentials-display";
import type { CredentialFormState } from "./index";
import { formatUsagePercent, type LiveUsageRow } from "./usage";

export function CredentialsTab({
  channel,
  credentials,
  statuses,
  form,
  onChangeForm,
  onEdit,
  onNew,
  onDelete,
  onSave,
  onUpdateStatus,
  supportsUsage,
  usageByCredential,
  usageRowsByCredential,
  usageLoadingByCredential,
  onQueryUsage,
  labels,
}: {
  channel: string;
  credentials: CredentialRow[];
  statuses: CredentialHealthRow[];
  form: CredentialFormState;
  onChangeForm: (patch: CredentialFormState) => void;
  onEdit: (row: CredentialRow) => void;
  onNew: () => void;
  onDelete: (row: CredentialRow) => void;
  onSave: () => void;
  onUpdateStatus: (
    row: { provider: string; index: number },
    status: "healthy" | "dead",
  ) => void;
  supportsUsage: boolean;
  usageByCredential: Record<number, string>;
  usageRowsByCredential: Record<number, LiveUsageRow[]>;
  usageLoadingByCredential: Record<number, boolean>;
  onQueryUsage: (row: CredentialRow) => void;
  labels: {
    title: string;
    add: string;
    replace: string;
    importJsonPlaceholder: string;
    none: string;
    edit: string;
    delete: string;
    showJson: string;
    hideJson: string;
    configured: string;
    statusNone: string;
    statusHealthy: string;
    statusCooldown: string;
    statusDead: string;
    expandJson: string;
    collapseJson: string;
    usageFetch: string;
    usageTitle: string;
    usageShow: string;
    usageHide: string;
    usageLimit: string;
    usagePercent: string;
    usageReset: string;
    usageRaw: string;
    usageEmpty: string;
    loading: string;
  };
}) {
  const fields = credentialFieldsForChannel(channel);
  const [expandedKey, setExpandedKey] = useState<string | null>(null);
  const [expandedUsageKey, setExpandedUsageKey] = useState<string | null>(null);
  const statusByIndex = new Map(statuses.map((row) => [row.index, row]));

  return (
    <div className="space-y-4">
      <div className="grid gap-4 lg:grid-cols-[1.1fr_0.9fr]">
        <Card
          title={labels.title}
          action={form.editingIndex !== null ? (
            <Button variant="neutral" onClick={onNew}>{labels.add}</Button>
          ) : undefined}
        >
          <div className="space-y-2">
            {credentials.length === 0 ? <p className="text-sm text-muted">{labels.none}</p> : null}
            {credentials.map((row) => {
              const credentialKey = `${row.provider}-${row.index}`;
              const expanded = expandedKey === credentialKey;
              const summary = summarizeCredential(row.credential);
              const status = statusByIndex.get(row.index) ?? null;
              const statusValue = status?.status ?? "healthy";
              const healthVariant =
                statusValue === "unavailable"
                  ? "danger"
                  : statusValue === "cooldown"
                    ? "accent"
                    : "success";
              const nextStatus =
                statusValue === "unavailable" || statusValue === "cooldown" ? "healthy" : "dead";
              const usageExpanded = expandedUsageKey === credentialKey;
              const usageRows = usageRowsByCredential[row.index] ?? [];
              const usageRaw = usageByCredential[row.index] ?? "";
              const usageLoading = Boolean(usageLoadingByCredential[row.index]);
              const usageActionLabels = getCredentialUsageActionLabels({
                expanded: usageExpanded,
                loading: usageLoading,
                labels: {
                  show: labels.usageShow,
                  hide: labels.usageHide,
                  refresh: labels.usageFetch,
                  loading: labels.loading,
                },
              });
              return (
                <div key={credentialKey} className="card-shell">
                  <div className="flex flex-wrap items-center gap-2">
                    <div className="font-semibold">#{row.index} · {summary.primary}</div>
                    <button
                      type="button"
                      className="badge-button"
                      onClick={() => onUpdateStatus(row, nextStatus)}
                    >
                      <Badge variant={healthVariant}>
                        {statusValue === "unavailable"
                          ? labels.statusDead
                          : statusValue === "cooldown"
                            ? labels.statusCooldown
                            : labels.statusHealthy}
                      </Badge>
                    </button>
                    <div className="ml-auto flex flex-wrap gap-2">
                      <Button variant="neutral" onClick={() => onEdit(row)}>
                        {labels.edit}
                      </Button>
                      <Button variant="danger" onClick={() => onDelete(row)}>
                        {labels.delete}
                      </Button>
                      {supportsUsage ? (
                        <Button
                          variant="neutral"
                          onClick={() => {
                            const nextExpanded = !usageExpanded;
                            setExpandedUsageKey(nextExpanded ? credentialKey : null);
                            if (nextExpanded && !usageRaw) {
                              onQueryUsage(row);
                            }
                          }}
                        >
                          {usageActionLabels.primary}
                        </Button>
                      ) : null}
                    </div>
                  </div>
                  {summary.secondary.length > 0 ? (
                    <div className="mt-2 text-xs text-muted">{summary.secondary.join(" · ")}</div>
                  ) : (
                    <div className="mt-2 text-xs text-muted">{labels.configured}</div>
                  )}
                  {expanded ? (
                    <pre className="mt-3 overflow-auto text-xs text-muted">
                      {JSON.stringify(row.credential, null, 2)}
                    </pre>
                  ) : null}
                  {supportsUsage && usageExpanded ? (
                    <div className="mt-4 space-y-3 rounded-lg border border-border px-3 py-3">
                      <div className="flex items-center justify-between gap-2">
                        <div className="text-xs font-semibold uppercase tracking-[0.08em] text-muted">
                          {labels.usageTitle}
                        </div>
                        <Button variant="neutral" onClick={() => onQueryUsage(row)}>
                          {usageActionLabels.refresh}
                        </Button>
                      </div>
                      {usageRows.length > 0 ? (
                        <div className="overflow-hidden rounded-lg border border-border">
                          <div className="grid grid-cols-[minmax(0,2fr)_90px_minmax(120px,1fr)] gap-2 border-b border-border px-3 py-2 text-xs font-semibold uppercase tracking-[0.08em] text-muted">
                            <span>{labels.usageLimit}</span>
                            <span>{labels.usagePercent}</span>
                            <span>{labels.usageReset}</span>
                          </div>
                          <div className="divide-y divide-border">
                            {usageRows.map((item) => (
                              <div
                                key={`${credentialKey}-${item.name}-${String(item.resetAt)}`}
                                className="grid grid-cols-[minmax(0,2fr)_90px_minmax(120px,1fr)] gap-2 px-3 py-2 text-xs text-text"
                              >
                                <span className="truncate">{item.name}</span>
                                <span>{formatUsagePercent(item.percent)}</span>
                                <span>
                                  {item.resetAt === null
                                    ? "—"
                                    : typeof item.resetAt === "number"
                                      ? new Date(item.resetAt).toLocaleString()
                                      : item.resetAt}
                                </span>
                              </div>
                            ))}
                          </div>
                        </div>
                      ) : (
                        <div className="text-xs text-muted">{labels.usageEmpty}</div>
                      )}
                      {usageRaw ? (
                        <details className="rounded-lg border border-border px-3 py-2">
                          <summary className="cursor-pointer text-xs font-semibold uppercase tracking-[0.08em] text-muted">
                            {labels.usageRaw}
                          </summary>
                          <pre className="mt-2 overflow-auto text-xs text-muted">{usageRaw}</pre>
                        </details>
                      ) : null}
                    </div>
                  ) : null}
                  <div className="mt-3 flex justify-end">
                    <button
                      type="button"
                      className="corner-toggle"
                      aria-label={expanded ? labels.collapseJson : labels.expandJson}
                      onClick={() => setExpandedKey(expanded ? null : credentialKey)}
                    >
                      {expanded ? "▾" : "▸"}
                    </button>
                  </div>
                </div>
              );
            })}
          </div>
        </Card>
        <Card title={form.editingIndex === null ? labels.add : labels.replace}>
          <div className="space-y-4">
            {form.editingIndex === null ? (
              <div>
                <TextArea
                  value={form.rawJson}
                  onChange={(value) =>
                    onChangeForm({ ...form, rawJson: value })
                  }
                  rows={8}
                  placeholder={labels.importJsonPlaceholder}
                />
              </div>
            ) : (
              <>
                {fields.map((field) => (
                  <div key={field.key}>
                    <Label>{field.label}</Label>
                    {field.type === "textarea" ? (
                      <TextArea
                        value={form.values[field.key] ?? ""}
                        onChange={(value) =>
                          onChangeForm({
                            ...form,
                            values: { ...form.values, [field.key]: value },
                          })
                        }
                        rows={4}
                      />
                    ) : (
                      <Input
                        value={form.values[field.key] ?? ""}
                        onChange={(value) =>
                          onChangeForm({
                            ...form,
                            values: { ...form.values, [field.key]: value },
                          })
                        }
                      />
                    )}
                  </div>
                ))}
              </>
            )}
            <Button onClick={onSave}>
              {form.editingIndex === null ? labels.add : labels.replace}
            </Button>
          </div>
        </Card>
      </div>

    </div>
  );
}
