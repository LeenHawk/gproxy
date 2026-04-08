import { useState } from "react";

import { Badge, Button, Card, Input, Label, TextArea } from "../../../components/ui";
import type { CredentialHealthRow, CredentialRow } from "../../../lib/types/admin";
import { credentialFieldsForChannel } from "./channel-forms";
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
    none: string;
    edit: string;
    delete: string;
    showJson: string;
    hideJson: string;
    configured: string;
    statusNone: string;
    statusHealthy: string;
    statusDead: string;
    statusAvailable: string;
    statusUnavailable: string;
    expandJson: string;
    collapseJson: string;
    usageFetch: string;
    usageTitle: string;
    usageLimit: string;
    usagePercent: string;
    usageReset: string;
    usageRaw: string;
    usageEmpty: string;
  };
}) {
  const fields = credentialFieldsForChannel(channel);
  const [expandedKey, setExpandedKey] = useState<string | null>(null);
  const [expandedUsageKey, setExpandedUsageKey] = useState<string | null>(null);
  const statusByIndex = new Map(statuses.map((row) => [row.index, row]));

  return (
    <div className="space-y-4">
      <div className="grid gap-4 lg:grid-cols-[1.1fr_0.9fr]">
        <Card title={labels.title}>
          <div className="space-y-2">
            {credentials.length === 0 ? <p className="text-sm text-muted">{labels.none}</p> : null}
            {credentials.map((row) => {
              const credentialKey = `${row.provider}-${row.index}`;
              const expanded = expandedKey === credentialKey;
              const summary = summarizeCredential(row.credential);
              const status = statusByIndex.get(row.index) ?? null;
              const healthVariant = status?.status === "dead" ? "danger" : "success";
              const availabilityVariant = status?.available === false ? "danger" : "accent";
              const nextStatus = status?.status === "dead" ? "healthy" : "dead";
              const usageExpanded = expandedUsageKey === credentialKey;
              const usageRows = usageRowsByCredential[row.index] ?? [];
              const usageRaw = usageByCredential[row.index] ?? "";
              const usageLoading = Boolean(usageLoadingByCredential[row.index]);
              return (
                <div key={credentialKey} className="card-shell">
                  <div className="flex items-start justify-between gap-3">
                    <div>
                      <div className="flex flex-wrap items-center gap-2">
                        <div className="font-semibold">#{row.index} · {summary.primary}</div>
                        <Badge variant={healthVariant}>
                          {status?.status === "dead" ? labels.statusDead : labels.statusHealthy}
                        </Badge>
                        <Badge variant={availabilityVariant}>
                          {status?.available === false ? labels.statusUnavailable : labels.statusAvailable}
                        </Badge>
                      </div>
                      {summary.secondary.length > 0 ? (
                        <div className="mt-1 text-xs text-muted">{summary.secondary.join(" · ")}</div>
                      ) : (
                        <div className="mt-1 text-xs text-muted">{labels.configured}</div>
                      )}
                      {expanded ? (
                        <pre className="mt-3 overflow-auto text-xs text-muted">
                          {JSON.stringify(row.credential, null, 2)}
                        </pre>
                      ) : null}
                    </div>
                    <div className="flex flex-wrap gap-2">
                      <Button
                        variant="neutral"
                        onClick={() => setExpandedKey(expanded ? null : credentialKey)}
                      >
                        {expanded ? "▾" : "▸"}
                      </Button>
                      <Button
                        variant={status?.status === "dead" ? "danger" : "primary"}
                        onClick={() => onUpdateStatus(row, nextStatus)}
                      >
                        {status?.status === "dead" ? labels.statusDead : labels.statusHealthy}
                      </Button>
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
                          {usageLoading ? labels.usageFetch : labels.usageTitle}
                        </Button>
                      ) : null}
                    </div>
                  </div>
                  {supportsUsage && usageExpanded ? (
                    <div className="mt-4 space-y-3 rounded-lg border border-border px-3 py-3">
                      <div className="flex items-center justify-between gap-2">
                        <div className="text-xs font-semibold uppercase tracking-[0.08em] text-muted">
                          {labels.usageTitle}
                        </div>
                        <Button variant="neutral" onClick={() => onQueryUsage(row)}>
                          {labels.usageFetch}
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
                </div>
              );
            })}
          </div>
        </Card>
        <Card title={form.editingIndex === null ? labels.add : labels.replace}>
          <div className="space-y-4">
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
            <Button onClick={onSave}>
              {form.editingIndex === null ? labels.add : labels.replace}
            </Button>
          </div>
        </Card>
      </div>

    </div>
  );
}
