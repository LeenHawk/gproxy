import { useCallback, useEffect, useMemo, useState } from "react";

import { useI18n } from "../../app/i18n";
import { apiRequest, formatError, isApiError } from "../../lib/api";
import { copyTextToClipboard } from "../../lib/clipboard";
import { parseRequiredI64, parseRequiredPositiveInteger } from "../../lib/form";
import type {
  CredentialQueryRow,
  CredentialStatusQueryRow,
  ProviderQueryRow
} from "../../lib/types";
import { Button, Card, ConfirmDialog } from "../../components/ui";
import { ConfigTab } from "./providers/ConfigTab";
import { CredentialsTab } from "./providers/CredentialsTab";
import { ProviderList } from "./providers/ProviderList";
import { useCredentialOAuth } from "./providers/hooks/useCredentialOAuth";
import { useCredentialStatus } from "./providers/hooks/useCredentialStatus";
import { useCredentialUsage } from "./providers/hooks/useCredentialUsage";
import { useProviderData } from "./providers/hooks/useProviderData";
import {
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
  getProviderChannelSelectOptions,
  isCustomChannel,
  normalizeChannel,
  normalizeDispatchRules,
  secretValuesFromSecretJson,
  supportsOAuth,
  supportsUpstreamUsage,
  toProviderFormState
} from "./providers";

type UpsertEntityAck = {
  ok: boolean;
  id: number;
};

type OpenAiModelListPayload = {
  data?: Array<{ id?: string | null }>;
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
  const [deleteTargetProvider, setDeleteTargetProvider] = useState<ProviderQueryRow | null>(null);
  const [deletingProvider, setDeletingProvider] = useState(false);
  const [deleteDeadCredentialIds, setDeleteDeadCredentialIds] = useState<number[] | null>(null);
  const [deletingDeadCredentials, setDeletingDeadCredentials] = useState(false);
  const [testingAllCredentials, setTestingAllCredentials] = useState(false);
  const [credentialForm, setCredentialForm] = useState(createEmptyCredentialFormState("custom"));

  const {
    providerRows,
    setProviderRows,
    selectedProviderId,
    setSelectedProviderId,
    channelCatalogRows,
    providerForm,
    setProviderForm,
    credentialRows,
    setCredentialRows,
    setStatusRows,
    selectedProvider,
    statusesByCredential,
    credentialSearchMode,
    credentialSearchText,
    credentialPageSize,
    credentialPage,
    credentialTotalCount,
    credentialFilteredCount,
    deadCredentialCount,
    credentialListLoading,
    setCredentialPage,
    updateCredentialSearchMode,
    updateCredentialSearchText,
    updateCredentialPageSize,
    loadProviders,
    loadProviderScopedData,
    loadAllProviderCredentials,
    loadDeadCredentialIds,
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
        await loadProviderScopedData(latestProvider);
        if (expected.size === 0) {
          return;
        }
        const allCredentials = await loadAllProviderCredentials(latestProvider);
        const idSet = new Set(allCredentials.map((row) => row.id));
        if (Array.from(expected).every((id) => idSet.has(id))) {
          return;
        }
        await new Promise<void>((resolve) => {
          window.setTimeout(resolve, 120 + attempt * 80);
        });
      }
    },
    [
      loadAllProviderCredentials,
      loadProviderScopedData,
      loadProviders,
      selectedProvider,
      selectedProviderId
    ]
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
    oauthActiveModeByCredential,
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
    ? supportsOAuth(selectedProvider.channel, channelCatalogRows)
    : false;
  const providerSupportsUpstreamUsage = selectedProvider
    ? supportsUpstreamUsage(selectedProvider.channel, channelCatalogRows)
    : false;
  const showWorkspace = isCreatingProvider || selectedProvider !== null;

  const providerFormChannel = normalizeChannel(providerForm.channel);
  const showCodexOAuthIssuer = providerFormChannel === "codex";
  const showOAuthTriplet =
    providerFormChannel === "geminicli" || providerFormChannel === "antigravity";
  const showVertexOAuthToken = providerFormChannel === "vertex";
  const showClaudeCodeSettings = providerFormChannel === "claudecode";
  const showClaudeTopLevelCacheControl =
    providerFormChannel === "anthropic" || providerFormChannel === "claudecode";
  const showCustomMaskTable = isCustomChannel(providerFormChannel);

  const channelOptions = useMemo(() => {
    const options = [...getProviderChannelSelectOptions(channelCatalogRows)];
    const current = providerForm.channel.trim();
    if (current && !options.some((item) => item.value === current)) {
      options.push({ value: current, label: `${current} (custom)` });
    }
    return options;
  }, [channelCatalogRows, providerForm.channel]);

  useEffect(() => {
    if (!showWorkspace && activeTab !== "config") {
      setActiveTab("config");
    }
  }, [activeTab, showWorkspace]);

  useEffect(() => {
    setDeleteDeadCredentialIds(null);
  }, [selectedProviderId]);

  useEffect(() => {
    resetUsageState();
    resetOAuthState();
    resetStatusEditor();
    setCredentialForm(
      createEmptyCredentialFormState(selectedProvider?.channel ?? providerForm.channel)
    );
  }, [providerForm.channel, resetOAuthState, resetStatusEditor, resetUsageState, selectedProvider]);

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
    setActiveTab("bulk");
  };

  const upsertProvider = async () => {
    try {
      const currentId =
        providerForm.id.trim() === "" ? null : parseRequiredI64(providerForm.id, "id");
      const rules = normalizeDispatchRules(providerForm.dispatchRules);
      const trimmedName = providerForm.name.trim();
      const trimmedChannel = providerForm.channel.trim();
      const dispatchJson = buildDispatchJson(rules);
      const settingsPayload = buildChannelSettingsJson(trimmedChannel, providerForm.settings);
      settingsPayload.credential_round_robin_enabled =
        providerForm.credentialRoundRobinEnabled;
      settingsPayload.credential_cache_affinity_enabled =
        providerForm.credentialRoundRobinEnabled &&
        providerForm.credentialCacheAffinityEnabled;
      settingsPayload.credential_cache_affinity_max_keys =
        parseRequiredPositiveInteger(
          providerForm.credentialCacheAffinityMaxKeys,
          "credential_cache_affinity_max_keys"
        );
      const saved = await apiRequest<UpsertEntityAck>("/admin/providers/upsert", {
        apiKey,
        method: "POST",
        body: {
          ...(currentId === null ? {} : { id: currentId }),
          name: trimmedName,
          channel: trimmedChannel,
          settings_json: JSON.stringify(settingsPayload),
          dispatch_json: JSON.stringify(dispatchJson),
          enabled: providerForm.enabled
        }
      });
      notify("success", t("providers.saved"));
      setIsCreatingProvider(false);
      setSelectedProviderId(saved.id);
      const latestProviders = await loadProviders();
      const savedRow = latestProviders.find((row) => row.id === saved.id);
      if (savedRow) {
        setProviderForm(toProviderFormState(savedRow));
      } else {
        setProviderForm((prev) => ({
          ...prev,
          id: String(saved.id),
          name: trimmedName,
          channel: trimmedChannel
        }));
      }
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

  const requestRemoveProvider = (row: ProviderQueryRow) => {
    setDeleteTargetProvider(row);
  };

  const confirmAndRemoveProvider = async () => {
    if (!deleteTargetProvider) {
      return;
    }

    setDeletingProvider(true);
    try {
      await removeProvider(deleteTargetProvider.id);
      setDeleteTargetProvider(null);
    } finally {
      setDeletingProvider(false);
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

  const applyDeletedCredentialIds = useCallback(
    (ids: number[]) => {
      if (ids.length === 0) {
        return;
      }
      const deletedIdSet = new Set(ids);
      setCredentialRows((prev) => prev.filter((row) => !deletedIdSet.has(row.id)));
      setStatusRows((prev) => prev.filter((row) => !deletedIdSet.has(row.credential_id)));
      for (const id of ids) {
        clearUsageForCredential(id);
      }
    },
    [clearUsageForCredential, setCredentialRows, setStatusRows]
  );

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
      applyDeletedCredentialIds([id]);
      notify("success", t("providers.credentials.deleted", { id }));
      window.setTimeout(() => {
        void loadProviderScopedData(selectedProvider);
      }, 250);
    } catch (error) {
      notify("error", formatError(error));
    }
  };

  const requestRemoveDeadCredentials = async () => {
    if (!selectedProvider || deadCredentialCount === 0) {
      return;
    }
    const ids = await loadDeadCredentialIds(selectedProvider);
    if (ids.length === 0) {
      return;
    }
    setDeleteDeadCredentialIds(ids);
  };

  const confirmAndRemoveDeadCredentials = async () => {
    if (!selectedProvider || !deleteDeadCredentialIds || deleteDeadCredentialIds.length === 0) {
      return;
    }

    const targetProvider = selectedProvider;
    const targetIds = deleteDeadCredentialIds.slice();
    setDeletingDeadCredentials(true);
    try {
      const results = await Promise.allSettled(
        targetIds.map(async (id) => {
          await apiRequest("/admin/credentials/delete", {
            apiKey,
            method: "POST",
            body: { id }
          });
          return id;
        })
      );

      const deletedIds: number[] = [];
      let failedCount = 0;

      for (const result of results) {
        if (result.status === "fulfilled") {
          deletedIds.push(result.value);
        } else {
          failedCount += 1;
        }
      }

      applyDeletedCredentialIds(deletedIds);
      setDeleteDeadCredentialIds(null);

      if (failedCount === 0) {
        notify(
          "success",
          t("providers.credentials.deadDeleted", { count: deletedIds.length })
        );
      } else {
        notify(
          "error",
          t("providers.credentials.deadDeletedPartial", {
            success: deletedIds.length,
            failed: failedCount
          })
        );
      }

      window.setTimeout(() => {
        void loadProviderScopedData(targetProvider);
      }, 250);
    } finally {
      setDeletingDeadCredentials(false);
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
    lastError,
    silent = false,
    reload = true
  }: {
    credentialId: number;
    statusId?: number;
    healthKind: "healthy" | "partial" | "dead";
    healthJson: Record<string, unknown> | null;
    lastError?: string | null;
    silent?: boolean;
    reload?: boolean;
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
      if (!silent) {
        notify("success", t("providers.status.saved"));
      }
      if (reload) {
        window.setTimeout(() => {
          void loadProviderScopedData(selectedProvider);
        }, 250);
      }
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

  const isLikelyHtmlErrorPage = (value: string): boolean => {
    const normalized = value.trim().toLowerCase();
    if (!normalized) {
      return false;
    }
    return (
      normalized.includes("<html") ||
      normalized.includes("<head") ||
      normalized.includes("<body") ||
      normalized.includes("<title>") ||
      normalized.includes("cloudflare")
    );
  };

  const isLikelyInvalidCredentialTestError = (error: unknown): boolean => {
    if (!isApiError(error)) {
      return false;
    }
    const normalized = error.message.trim().toLowerCase();
    if (!normalized) {
      return false;
    }
    if (error.status === 401) {
      return true;
    }
    if (error.status === 403 && !isLikelyHtmlErrorPage(error.message)) {
      return true;
    }
    return [
      "last_status=some(401)",
      "last_status=some(403)",
      "invalid credential",
      "invalid_grant",
      "invalid_client",
      "unauthorized_client",
      "refresh_token_expired",
      "refresh_token_reused",
      "refresh_token_invalidated",
      "missing refresh token",
      "oauth token response missing refresh_token",
      "upstream status 401 after",
      "upstream status 403 after"
    ].some((pattern) => normalized.includes(pattern));
  };

  const classifyCredentialTestError = (
    error: unknown
  ): {
    healthKind: "partial" | "dead";
    healthJson: Record<string, unknown> | null;
    lastError: string;
  } => {
    const lastError = formatError(error);
    if (isApiError(error)) {
      if (error.status === 403 && isLikelyHtmlErrorPage(error.message)) {
        return {
          healthKind: "partial",
          healthJson: { models: [] },
          lastError
        };
      }
      if (isLikelyInvalidCredentialTestError(error)) {
        return {
          healthKind: "dead",
          healthJson: null,
          lastError
        };
      }
      if ([429, 500, 502, 503, 504].includes(error.status)) {
        return {
          healthKind: "partial",
          healthJson: { models: [] },
          lastError
        };
      }
    }
    return {
      healthKind: "dead",
      healthJson: null,
      lastError
    };
  };

  const testAllCredentials = async () => {
    if (!selectedProvider) {
      return;
    }
    if (credentialTotalCount === 0) {
      notify("info", t("providers.credentials.testAllEmpty"));
      return;
    }
    const allCredentialRows = await loadAllProviderCredentials(selectedProvider);
    if (allCredentialRows.length === 0) {
      return;
    }

    setTestingAllCredentials(true);
    let availableCount = 0;
    let unavailableCount = 0;

    try {
      for (const row of allCredentialRows) {
        const statusId = (statusesByCredential.get(row.id) ?? [])[0]?.id;

        try {
          const modelList = await apiRequest<OpenAiModelListPayload>(
            `/${providerRouteKey}/v1/models?credential_id=${row.id}`,
            { apiKey }
          );
          const modelId = modelList.data?.find((item) => typeof item.id === "string" && item.id.trim())?.id?.trim();
          if (!modelId) {
            throw new Error(t("providers.credentials.testAllNoModel"));
          }

          await apiRequest(`/${providerRouteKey}/v1/chat/completions?credential_id=${row.id}`, {
            apiKey,
            method: "POST",
            body: {
              model: modelId,
              messages: [{ role: "user", content: "test" }],
              max_tokens: 1,
              stream: false
            }
          });

          await upsertCredentialHealthState({
            credentialId: row.id,
            statusId,
            healthKind: "healthy",
            healthJson: null,
            lastError: null,
            silent: true,
            reload: false
          });
          availableCount += 1;
        } catch (error) {
          const failureState = classifyCredentialTestError(error);
          await upsertCredentialHealthState({
            credentialId: row.id,
            statusId,
            healthKind: failureState.healthKind,
            healthJson: failureState.healthJson,
            lastError: failureState.lastError,
            silent: true,
            reload: false
          });
          unavailableCount += 1;
        }
      }

      notify(
        availableCount === allCredentialRows.length ? "success" : "info",
        t("providers.credentials.testAllDone", {
          total: allCredentialRows.length,
          available: availableCount,
          unavailable: unavailableCount
        })
      );
      await loadProviderScopedData(selectedProvider);
    } finally {
      setTestingAllCredentials(false);
    }
  };

  const credentialsViewModel = {
    selectedProvider,
    credentialSchema: currentCredentialSchema,
    supportsUpstreamUsage: providerSupportsUpstreamUsage,
    supportsOAuth: providerSupportsOAuth,
    deadCredentialCount,
    testingAllCredentials,
    credentialTotalCount,
    credentialFilteredCount,
    credentialPageSize,
    credentialPage,
    credentialSearchMode,
    credentialSearchText,
    credentialListLoading,
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
    oauthActiveModeByCredential,
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
    onChangeCredentialSearchMode: updateCredentialSearchMode,
    onChangeCredentialSearchText: updateCredentialSearchText,
    onChangeCredentialPageSize: updateCredentialPageSize,
    onChangeCredentialPage: (page: number) => setCredentialPage(page),
    onEditCredential: editCredential,
    onCopyCredential: (row: CredentialQueryRow) => void copyCredential(row),
    onRemoveCredential: (id: number) => void removeCredential(id),
    onRequestRemoveDeadCredentials: () => void requestRemoveDeadCredentials(),
    onTestAllCredentials: () => void testAllCredentials(),
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
            onDelete={requestRemoveProvider}
            t={t}
          />

          {!showWorkspace ? (
            <div className="provider-card text-sm text-muted">{t("providers.selectHint")}</div>
          ) : (
            <div className="space-y-4">
              <div className="flex flex-wrap gap-2">
                {([
                  {
                    id: "bulk",
                    label: t("providers.tab.bulk"),
                    enabled: !!selectedProvider
                  },
                  {
                    id: "oauth",
                    label: t("providers.tab.oauth"),
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

              {activeTab === "bulk" ? (
                <CredentialsTab
                  mode="bulk"
                  viewModel={credentialsViewModel}
                  actions={credentialsActions}
                  t={t}
                />
              ) : null}

              {activeTab === "oauth" ? (
                <CredentialsTab
                  mode="oauth"
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
      <ConfirmDialog
        open={deleteTargetProvider !== null}
        title={t("providers.deleteConfirmTitle")}
        description={
          deleteTargetProvider
            ? t("providers.deleteConfirm", {
                id: deleteTargetProvider.id,
                name: deleteTargetProvider.name
              })
            : ""
        }
        cancelLabel={t("common.cancel")}
        confirmLabel={t("common.delete")}
        busy={deletingProvider}
        onClose={() => {
          if (!deletingProvider) {
            setDeleteTargetProvider(null);
          }
        }}
        onConfirm={() => void confirmAndRemoveProvider()}
      />
      <ConfirmDialog
        open={deleteDeadCredentialIds !== null}
        title={t("providers.credentials.deleteDeadConfirmTitle")}
        description={
          selectedProvider && deleteDeadCredentialIds
            ? t("providers.credentials.deleteDeadConfirm", {
                name: selectedProvider.name,
                count: deleteDeadCredentialIds.length
              })
            : ""
        }
        cancelLabel={t("common.cancel")}
        confirmLabel={t("common.delete")}
        busy={deletingDeadCredentials}
        onClose={() => {
          if (!deletingDeadCredentials) {
            setDeleteDeadCredentialIds(null);
          }
        }}
        onConfirm={() => void confirmAndRemoveDeadCredentials()}
      />
    </div>
  );
}
