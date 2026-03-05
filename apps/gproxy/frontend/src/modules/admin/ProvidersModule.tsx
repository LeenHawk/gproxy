import { useCallback, useEffect, useMemo, useState } from "react";

import { useI18n } from "../../app/i18n";
import { apiRequest, formatError } from "../../lib/api";
import { copyTextToClipboard } from "../../lib/clipboard";
import { parseRequiredI64 } from "../../lib/form";
import type {
  CredentialQueryRow,
  CredentialStatusQueryRow,
  ProviderQueryRow
} from "../../lib/types";
import { Button, Card } from "../../components/ui";
import { ConfigTab } from "./providers/ConfigTab";
import { CredentialsTab } from "./providers/CredentialsTab";
import { ProviderList } from "./providers/ProviderList";
import { useCredentialOAuth } from "./providers/hooks/useCredentialOAuth";
import { useCredentialStatus } from "./providers/hooks/useCredentialStatus";
import { useCredentialUsage } from "./providers/hooks/useCredentialUsage";
import { useProviderData } from "./providers/hooks/useProviderData";
import {
  CHANNEL_SELECT_OPTIONS,
  type BulkCredentialImportEntry,
  type DispatchRuleDraft,
  type WorkspaceTab,
  buildChannelSettingsJson,
  buildCredentialSecretJson,
  credentialDefaultNameFromSecretValues,
  buildDispatchJson,
  createEmptyCredentialFormState,
  createDefaultDispatchRule,
  credentialFormFromRow,
  credentialSchemaForChannel,
  isCustomChannel,
  normalizeChannel,
  normalizeDispatchRules,
  secretValuesFromSecretJson,
  supportsOAuth,
  supportsUpstreamUsage
} from "./providers";

type UpsertEntityAck = {
  ok: boolean;
  id: number;
};

export function ProvidersModule({
  apiKey,
  notify
}: {
  apiKey: string;
  notify: (kind: "success" | "error" | "info", message: string) => void;
}) {
  const { t } = useI18n();
  const [activeTab, setActiveTab] = useState<WorkspaceTab>("config");
  const [isCreatingProvider, setIsCreatingProvider] = useState(false);
  const [credentialForm, setCredentialForm] = useState(createEmptyCredentialFormState("custom"));

  const {
    providerRows,
    setProviderRows,
    selectedProviderId,
    setSelectedProviderId,
    providerForm,
    setProviderForm,
    credentialRows,
    setCredentialRows,
    setStatusRows,
    selectedProvider,
    statusesByCredential,
    loadProviders,
    loadProviderScopedData,
    beginCreateProvider: beginCreateProviderData,
    selectProvider: selectProviderData
  } = useProviderData({
    apiKey,
    notify
  });

  const providerRouteKey = selectedProvider ? encodeURIComponent(selectedProvider.channel) : "";

  const refreshProviderScopedData = useCallback(
    async (expectedCredentialIds?: number[]) => {
      const expected = new Set(
        (expectedCredentialIds ?? []).filter((id) => Number.isInteger(id) && id >= 0)
      );
      const targetProviderId = selectedProviderId ?? selectedProvider?.id ?? null;
      for (let attempt = 0; attempt < 8; attempt += 1) {
        const latestProviders = await loadProviders();
        const latestProvider =
          targetProviderId === null
            ? selectedProvider
            : latestProviders.find((row) => row.id === targetProviderId) ?? selectedProvider;
        const { credentials } = await loadProviderScopedData(latestProvider);
        if (expected.size === 0) {
          return;
        }
        const idSet = new Set(credentials.map((row) => row.id));
        if (Array.from(expected).every((id) => idSet.has(id))) {
          return;
        }
        await new Promise<void>((resolve) => {
          window.setTimeout(resolve, 120 + attempt * 80);
        });
      }
    },
    [loadProviderScopedData, loadProviders, selectedProvider, selectedProviderId]
  );

  const {
    usageByCredential,
    liveUsageRowsByCredential,
    usageDisplayKindByCredential,
    usageDisplayRowsByCredential,
    usageLoadingByCredential,
    usageErrorByCredential,
    queryUpstreamUsage,
    clearUsageForCredential,
    resetUsageState
  } = useCredentialUsage({
    apiKey,
    notify,
    t,
    selectedProvider,
    providerRouteKey
  });

  const {
    oauthStartQueryByCredential,
    setOauthStartQueryByCredential,
    oauthCallbackQueryByCredential,
    setOauthCallbackQueryByCredential,
    oauthResultByCredential,
    runCredentialOAuthStart,
    runCredentialOAuthCallback,
    resetOAuthState
  } = useCredentialOAuth({
    apiKey,
    notify,
    t,
    selectedProvider,
    providerRouteKey,
    loadProviderScopedData,
    refreshProviderScopedData: () => refreshProviderScopedData()
  });

  const {
    statusEditorCredentialId,
    setStatusEditorCredentialId,
    statusForm,
    setStatusForm,
    upsertStatus,
    resetStatusEditor
  } = useCredentialStatus({
    apiKey,
    notify,
    t,
    selectedProvider,
    loadProviderScopedData
  });

  const currentCredentialSchema = credentialSchemaForChannel(
    selectedProvider?.channel ?? providerForm.channel
  );
  const providerSupportsOAuth = selectedProvider
    ? supportsOAuth(selectedProvider.channel)
    : false;
  const providerSupportsUpstreamUsage = selectedProvider
    ? supportsUpstreamUsage(selectedProvider.channel)
    : false;
  const showWorkspace = isCreatingProvider || selectedProvider !== null;

  const providerFormChannel = normalizeChannel(providerForm.channel);
  const showCodexOAuthIssuer = providerFormChannel === "codex";
  const showOAuthTriplet =
    providerFormChannel === "geminicli" || providerFormChannel === "antigravity";
  const showVertexOAuthToken = providerFormChannel === "vertex";
  const showClaudeCodeSettings = providerFormChannel === "claudecode";
  const showClaudeTopLevelCacheControl =
    providerFormChannel === "claude" || providerFormChannel === "claudecode";
  const showCustomMaskTable = isCustomChannel(providerFormChannel);

  const channelOptions = useMemo(() => {
    const options = [...CHANNEL_SELECT_OPTIONS];
    const current = providerForm.channel.trim();
    if (current && !options.some((item) => item.value === current)) {
      options.push({ value: current, label: `${current} (custom)` });
    }
    return options;
  }, [providerForm.channel]);

  useEffect(() => {
    if (!showWorkspace && activeTab !== "config") {
      setActiveTab("config");
    }
  }, [activeTab, showWorkspace]);

  useEffect(() => {
    void loadProviderScopedData(selectedProvider);
    resetUsageState();
    resetOAuthState();
    resetStatusEditor();
    setCredentialForm(
      createEmptyCredentialFormState(selectedProvider?.channel ?? providerForm.channel)
    );
  }, [
    loadProviderScopedData,
    resetOAuthState,
    resetStatusEditor,
    resetUsageState,
    selectedProvider
  ]);

  const beginCreateProvider = () => {
    beginCreateProviderData();
    setIsCreatingProvider(true);
    setActiveTab("config");
  };

  const editProvider = (row: ProviderQueryRow) => {
    selectProviderData(row);
    setIsCreatingProvider(false);
    setActiveTab("config");
  };

  const selectProvider = (row: ProviderQueryRow) => {
    selectProviderData(row);
    setIsCreatingProvider(false);
    setActiveTab("config");
  };

  const upsertProvider = async () => {
    try {
      const currentId =
        providerForm.id.trim() === "" ? null : parseRequiredI64(providerForm.id, "id");
      const rules = normalizeDispatchRules(providerForm.dispatchRules);
      const dispatchJson = buildDispatchJson(rules);
      const settingsPayload = buildChannelSettingsJson(providerForm.channel, providerForm.settings);
      settingsPayload.credential_round_robin_enabled =
        providerForm.credentialRoundRobinEnabled;
      settingsPayload.credential_cache_affinity_enabled =
        providerForm.credentialRoundRobinEnabled &&
        providerForm.credentialCacheAffinityEnabled;
      const saved = await apiRequest<UpsertEntityAck>("/admin/providers/upsert", {
        apiKey,
        method: "POST",
        body: {
          ...(currentId === null ? {} : { id: currentId }),
          name: providerForm.name.trim(),
          channel: providerForm.channel.trim(),
          settings_json: JSON.stringify(settingsPayload),
          dispatch_json: JSON.stringify(dispatchJson),
          enabled: providerForm.enabled
        }
      });
      notify("success", t("providers.saved"));
      setIsCreatingProvider(false);
      setSelectedProviderId(saved.id);
      await loadProviders();
    } catch (error) {
      notify("error", formatError(error));
    }
  };

  const removeProvider = async (id: number) => {
    try {
      await apiRequest("/admin/providers/delete", {
        apiKey,
        method: "POST",
        body: { id }
      });
      notify("success", t("providers.deleted", { id }));
      if (selectedProviderId === id) {
        setSelectedProviderId(null);
      }
      await loadProviders();
    } catch (error) {
      notify("error", formatError(error));
    }
  };

  const toggleProviderEnabled = async (row: ProviderQueryRow) => {
    const nextEnabled = !row.enabled;
    setProviderRows((prev) =>
      prev.map((item) => (item.id === row.id ? { ...item, enabled: nextEnabled } : item))
    );
    if (selectedProviderId === row.id) {
      setProviderForm((prev) => ({ ...prev, enabled: nextEnabled }));
    }

    try {
      await apiRequest("/admin/providers/upsert", {
        apiKey,
        method: "POST",
        body: {
          id: row.id,
          name: row.name,
          channel: row.channel,
          settings_json: JSON.stringify(row.settings_json),
          dispatch_json: JSON.stringify(row.dispatch_json),
          enabled: nextEnabled
        }
      });
      notify(
        "success",
        t("providers.enabledChanged", {
          id: row.id,
          state: nextEnabled ? t("common.enabled") : t("common.disabled")
        })
      );
      window.setTimeout(() => {
        void loadProviders();
      }, 250);
    } catch (error) {
      setProviderRows((prev) =>
        prev.map((item) => (item.id === row.id ? { ...item, enabled: row.enabled } : item))
      );
      if (selectedProviderId === row.id) {
        setProviderForm((prev) => ({ ...prev, enabled: row.enabled }));
      }
      notify("error", formatError(error));
    }
  };

  const addDispatchRule = () => {
    setProviderForm((prev) => ({
      ...prev,
      dispatchRules: [...prev.dispatchRules, createDefaultDispatchRule()]
    }));
  };

  const updateDispatchRule = (
    id: string,
    patch: Partial<Omit<DispatchRuleDraft, "id">>
  ) => {
    setProviderForm((prev) => ({
      ...prev,
      dispatchRules: prev.dispatchRules.map((rule) =>
        rule.id === id ? { ...rule, ...patch } : rule
      )
    }));
  };

  const removeDispatchRule = (id: string) => {
    setProviderForm((prev) => {
      const next = prev.dispatchRules.filter((rule) => rule.id !== id);
      return {
        ...prev,
        dispatchRules: next.length ? next : [createDefaultDispatchRule()]
      };
    });
  };

  const upsertCredential = async () => {
    if (!selectedProvider) {
      notify("error", t("providers.needProvider"));
      return;
    }
    try {
      const existingId =
        credentialForm.id.trim() === "" ? null : parseRequiredI64(credentialForm.id, "id");
      const secretJson = buildCredentialSecretJson(
        selectedProvider.channel,
        credentialForm.secretValues
      );
      const resolvedName =
        credentialForm.name.trim() ||
        credentialDefaultNameFromSecretValues(
          selectedProvider.channel,
          credentialForm.secretValues
        );
      const saved = await apiRequest<UpsertEntityAck>("/admin/credentials/upsert", {
        apiKey,
        method: "POST",
        body: {
          ...(existingId === null ? {} : { id: existingId }),
          provider_id: selectedProvider.id,
          name: resolvedName,
          kind: currentCredentialSchema.kind,
          settings_json: credentialForm.settingsPayload
            ? JSON.stringify(credentialForm.settingsPayload)
            : null,
          secret_json: secretJson,
          enabled: credentialForm.enabled
        }
      });
      const now = new Date().toISOString();
      setCredentialRows((prev) => {
        const next = prev.slice();
        const id = saved.id;
        const index = next.findIndex((row) => row.id === id);
        const row: CredentialQueryRow = {
          id,
          provider_id: selectedProvider.id,
          name: resolvedName,
          kind: currentCredentialSchema.kind,
          settings_json: credentialForm.settingsPayload ?? null,
          secret_json: JSON.parse(secretJson) as Record<string, unknown>,
          enabled: credentialForm.enabled,
          created_at: index >= 0 ? next[index].created_at : now,
          updated_at: now
        };
        if (index >= 0) {
          next[index] = row;
        } else {
          next.unshift(row);
        }
        return next;
      });
      notify("success", t("providers.credentials.saved"));
      await refreshProviderScopedData([saved.id]);
    } catch (error) {
      notify("error", formatError(error));
    }
  };

  const upsertCredentialsBatch = async (entries: BulkCredentialImportEntry[]) => {
    if (!selectedProvider) {
      notify("error", t("providers.needProvider"));
      return;
    }
    if (entries.length === 0) {
      notify("error", t("providers.bulk.emptyImport"));
      return;
    }

    try {
      const importedIds: number[] = [];

      for (const entry of entries) {
        const secretJson = buildCredentialSecretJson(
          selectedProvider.channel,
          entry.secretValues
        );
        const resolvedName =
          entry.name?.trim() ||
          credentialDefaultNameFromSecretValues(
            selectedProvider.channel,
            entry.secretValues
          );

        const saved = await apiRequest<UpsertEntityAck>("/admin/credentials/upsert", {
          apiKey,
          method: "POST",
          body: {
            ...(typeof entry.id === "number" ? { id: entry.id } : {}),
            provider_id: selectedProvider.id,
            name: resolvedName,
            kind: currentCredentialSchema.kind,
            settings_json: entry.settingsPayload
              ? JSON.stringify(entry.settingsPayload)
              : null,
            secret_json: secretJson,
            enabled: entry.enabled ?? true
          }
        });
        importedIds.push(saved.id);
      }

      notify("success", t("providers.bulk.imported", { count: entries.length }));
      await refreshProviderScopedData(importedIds);
    } catch (error) {
      notify("error", formatError(error));
    }
  };

  const removeCredential = async (id: number) => {
    if (!selectedProvider) {
      return;
    }
    try {
      await apiRequest("/admin/credentials/delete", {
        apiKey,
        method: "POST",
        body: { id }
      });
      setCredentialRows((prev) => prev.filter((row) => row.id !== id));
      setStatusRows((prev) => prev.filter((row) => row.credential_id !== id));
      clearUsageForCredential(id);
      notify("success", t("providers.credentials.deleted", { id }));
      window.setTimeout(() => {
        void loadProviderScopedData(selectedProvider);
      }, 250);
    } catch (error) {
      notify("error", formatError(error));
    }
  };

  const toggleCredentialEnabled = async (row: CredentialQueryRow) => {
    if (!selectedProvider) {
      return;
    }
    const nextEnabled = !row.enabled;
    setCredentialRows((prev) =>
      prev.map((item) => (item.id === row.id ? { ...item, enabled: nextEnabled } : item))
    );
    try {
      await apiRequest("/admin/credentials/upsert", {
        apiKey,
        method: "POST",
        body: {
          id: row.id,
          provider_id: row.provider_id,
          name: row.name,
          kind: row.kind,
          settings_json: row.settings_json ? JSON.stringify(row.settings_json) : null,
          secret_json: JSON.stringify(row.secret_json),
          enabled: nextEnabled
        }
      });
      notify(
        "success",
        t("providers.credentials.enabledChanged", {
          id: row.id,
          state: nextEnabled ? t("common.enabled") : t("common.disabled")
        })
      );
      window.setTimeout(() => {
        void loadProviderScopedData(selectedProvider);
      }, 250);
    } catch (error) {
      setCredentialRows((prev) =>
        prev.map((item) => (item.id === row.id ? { ...item, enabled: row.enabled } : item))
      );
      notify("error", formatError(error));
    }
  };

  const upsertCredentialHealthState = async ({
    credentialId,
    statusId,
    healthKind,
    healthJson,
    lastError
  }: {
    credentialId: number;
    statusId?: number;
    healthKind: "healthy" | "partial" | "dead";
    healthJson: Record<string, unknown> | null;
    lastError?: string | null;
  }) => {
    if (!selectedProvider) {
      return;
    }
    const checkedAtMs = Date.now();
    try {
      await apiRequest("/admin/credential-statuses/upsert", {
        apiKey,
        method: "POST",
        body: {
          id: statusId,
          credential_id: credentialId,
          channel: selectedProvider.channel,
          health_kind: healthKind,
          health_json: healthJson ? JSON.stringify(healthJson) : null,
          checked_at_unix_ms: checkedAtMs,
          last_error: lastError ?? null
        }
      });
      const nowIso = new Date(checkedAtMs).toISOString();
      setStatusRows((prev) => {
        const next = prev.slice();
        const index = next.findIndex(
          (row) =>
            row.credential_id === credentialId && row.channel === selectedProvider.channel
        );
        const base = index >= 0 ? next[index] : undefined;
        const nextRow: CredentialStatusQueryRow = {
          id: statusId ?? base?.id ?? -credentialId,
          credential_id: credentialId,
          channel: selectedProvider.channel,
          health_kind: healthKind,
          health_json: healthJson,
          checked_at: nowIso,
          last_error: lastError ?? null,
          updated_at: nowIso
        };
        if (index >= 0) {
          next[index] = nextRow;
        } else {
          next.push(nextRow);
        }
        return next;
      });
      notify("success", t("providers.status.saved"));
      window.setTimeout(() => {
        void loadProviderScopedData(selectedProvider);
      }, 250);
    } catch (error) {
      notify("error", formatError(error));
    }
  };

  const editCredential = (row: CredentialQueryRow) => {
    setCredentialForm(
      credentialFormFromRow(selectedProvider?.channel ?? providerForm.channel, row)
    );
    setActiveTab("list");
  };

  const copyCredential = async (row: CredentialQueryRow) => {
    const channel = selectedProvider?.channel ?? providerForm.channel;
    const isKeyChannel =
      currentCredentialSchema.fields.length === 1 &&
      currentCredentialSchema.fields[0]?.key === "api_key";
    const unwrapSecretJson = (value: Record<string, unknown>): Record<string, unknown> => {
      const custom = value.Custom;
      if (custom && typeof custom === "object" && !Array.isArray(custom)) {
        return custom as Record<string, unknown>;
      }
      const builtin = value.Builtin;
      if (builtin && typeof builtin === "object" && !Array.isArray(builtin)) {
        const entries = Object.values(builtin as Record<string, unknown>);
        if (
          entries.length === 1 &&
          entries[0] &&
          typeof entries[0] === "object" &&
          !Array.isArray(entries[0])
        ) {
          return entries[0] as Record<string, unknown>;
        }
      }
      return value;
    };
    const copiedText = (() => {
      if (!isKeyChannel) {
        return JSON.stringify(unwrapSecretJson(row.secret_json));
      }
      const secretValues = secretValuesFromSecretJson(channel, row.secret_json);
      const key = secretValues.api_key;
      if (typeof key === "string" && key.trim()) {
        return key.trim();
      }
      return JSON.stringify(row.secret_json);
    })();
    try {
      await copyTextToClipboard(copiedText);
      notify("success", t("common.copied"));
    } catch {
      notify("error", t("common.copyFailed"));
    }
  };

  const queryUpstreamUsageAndReload = async (credentialId: number) => {
    await queryUpstreamUsage(credentialId);
    await loadProviderScopedData(selectedProvider);
  };

  const credentialsViewModel = {
    selectedProvider,
    credentialSchema: currentCredentialSchema,
    supportsUpstreamUsage: providerSupportsUpstreamUsage,
    supportsOAuth: providerSupportsOAuth,
    credentialRows,
    statusesByCredential,
    usageByCredential,
    liveUsageRowsByCredential,
    usageDisplayKindByCredential,
    usageDisplayRowsByCredential,
    usageLoadingByCredential,
    usageErrorByCredential,
    oauthStartQueryByCredential,
    oauthCallbackQueryByCredential,
    oauthResultByCredential,
    statusEditorCredentialId,
    statusForm,
    credentialForm
  };

  const credentialsActions = {
    setOauthStartQueryByCredential,
    setOauthCallbackQueryByCredential,
    setStatusEditorCredentialId,
    setStatusForm,
    setCredentialForm,
    onEditCredential: editCredential,
    onCopyCredential: (row: CredentialQueryRow) => void copyCredential(row),
    onRemoveCredential: (id: number) => void removeCredential(id),
    onToggleCredentialEnabled: (row: CredentialQueryRow) => void toggleCredentialEnabled(row),
    onSetCredentialHealth: (payload: {
      credentialId: number;
      statusId?: number;
      healthKind: "healthy" | "partial" | "dead";
      healthJson: Record<string, unknown> | null;
      lastError?: string | null;
    }) => void upsertCredentialHealthState(payload),
    onQueryUpstreamUsage: (id: number) => void queryUpstreamUsageAndReload(id),
    onUpsertStatus: () => void upsertStatus(),
    onRunCredentialOAuthStart: (
      id?: number,
      mode?: string,
      queryDefaults?: Record<string, string | null | undefined>
    ) => void runCredentialOAuthStart(id, mode, queryDefaults),
    onRunCredentialOAuthCallback: (
      id?: number,
      mode?: string,
      queryDefaults?: Record<string, string | null | undefined>
    ) => void runCredentialOAuthCallback(id, mode, queryDefaults),
    onUpsertCredential: () => void upsertCredential(),
    onUpsertCredentialsBatch: (entries: BulkCredentialImportEntry[]) =>
      void upsertCredentialsBatch(entries)
  };

  return (
    <div className="space-y-4">
      <Card
        title={t("providers.title")}
        subtitle={t("providers.subtitle")}
        action={
          <div className="flex flex-wrap gap-2">
            <Button variant="neutral" onClick={() => void loadProviders()}>
              {t("providers.refresh")}
            </Button>
            <Button onClick={beginCreateProvider}>{t("providers.create")}</Button>
          </div>
        }
      >
        <div className="space-y-4">
          <ProviderList
            providerRows={providerRows}
            selectedProviderId={selectedProviderId}
            onSelectProvider={selectProvider}
            onToggleEnabled={(row) => void toggleProviderEnabled(row)}
            onEdit={editProvider}
            onDelete={(id) => void removeProvider(id)}
            t={t}
          />

          {!showWorkspace ? (
            <div className="provider-card text-sm text-muted">{t("providers.selectHint")}</div>
          ) : (
            <div className="space-y-4">
              <div className="flex flex-wrap gap-2">
                {([
                  {
                    id: "single",
                    label: t("providers.tab.single"),
                    enabled: !!selectedProvider
                  },
                  {
                    id: "bulk",
                    label: t("providers.tab.bulk"),
                    enabled: !!selectedProvider
                  },
                  {
                    id: "list",
                    label: t("providers.tab.list"),
                    enabled: !!selectedProvider
                  },
                  { id: "config", label: t("providers.tab.config"), enabled: true },
                ] as Array<{ id: WorkspaceTab; label: string; enabled: boolean }>).map((tab) => (
                  <button
                    key={tab.id}
                    type="button"
                    className={`workspace-tab ${activeTab === tab.id ? "workspace-tab-active" : ""}`}
                    disabled={!tab.enabled}
                    onClick={() => setActiveTab(tab.id)}
                  >
                    {tab.label}
                  </button>
                ))}
              </div>

              {activeTab === "config" ? (
                <ConfigTab
                  providerForm={providerForm}
                  setProviderForm={setProviderForm}
                  channelOptions={channelOptions}
                  showCodexOAuthIssuer={showCodexOAuthIssuer}
                  showOAuthTriplet={showOAuthTriplet}
                  showVertexOAuthToken={showVertexOAuthToken}
                  showClaudeCodeSettings={showClaudeCodeSettings}
                  showClaudeTopLevelCacheControl={showClaudeTopLevelCacheControl}
                  showCustomMaskTable={showCustomMaskTable}
                  addDispatchRule={addDispatchRule}
                  updateDispatchRule={updateDispatchRule}
                  removeDispatchRule={removeDispatchRule}
                  isCreatingProvider={isCreatingProvider}
                  onCancelCreate={() => setIsCreatingProvider(false)}
                  onSave={() => void upsertProvider()}
                  t={t}
                />
              ) : null}

              {activeTab === "single" ? (
                <CredentialsTab
                  mode="single"
                  viewModel={credentialsViewModel}
                  actions={credentialsActions}
                  t={t}
                />
              ) : null}

              {activeTab === "bulk" ? (
                <CredentialsTab
                  mode="bulk"
                  viewModel={credentialsViewModel}
                  actions={credentialsActions}
                  t={t}
                />
              ) : null}

              {activeTab === "list" ? (
                <CredentialsTab
                  mode="list"
                  viewModel={credentialsViewModel}
                  actions={credentialsActions}
                  t={t}
                />
              ) : null}
            </div>
          )}
        </div>
      </Card>
    </div>
  );
}
