import { useEffect, useMemo, useState } from "react";

import { useI18n } from "../../app/i18n";
import { Button } from "../../components/ui";
import { useBatchSelection } from "../../components/useBatchSelection";
import { apiJson, apiVoid } from "../../lib/api";
import { authHeaders } from "../../lib/auth";
import { parseRequiredI64 } from "../../lib/form";
import type {
  CredentialRow,
  DispatchTableDocument,
  OAuthCallbackResponse,
  OAuthStartResponse,
  ProviderDispatchTemplateParams,
  ProviderWrite,
} from "../../lib/types/admin";
import {
  buildChannelSettingsJson,
  buildCredentialJson,
  credentialValuesFromJson,
  defaultSettingsForChannel,
  emptyCredentialValuesForChannel,
} from "./providers/channel-forms";
import {
  buildDispatchDocument,
  createDispatchRuleDraft,
  dispatchDraftsFromDocument,
} from "./providers/dispatch";
import { buildOAuthCallbackQuery } from "./providers/oauth";
import { ConfigTab } from "./providers/ConfigTab";
import { CredentialsTab } from "./providers/CredentialsTab";
import { ModelsTab, type ModelFormState } from "./providers/ModelsTab";
import { OAuthTab } from "./providers/OAuthTab";
import { RewriteRulesTab } from "./providers/RewriteRulesTab";
import { ProviderList } from "./providers/ProviderList";
import {
  filterModelsForProvider,
  nextResourceId,
} from "./providers/resources";
import { useProviderData } from "./providers/hooks/useProviderData";
import type { CredentialFormState, ProviderWorkspaceTab } from "./providers";
import { parseLiveUsageRows, supportsCredentialUsageChannel, type LiveUsageRow } from "./providers/usage";
import type { MemoryModelRow, ModelWrite } from "../../lib/types/admin";

export function ProvidersModule({
  sessionToken,
  notify,
}: {
  sessionToken: string;
  notify: (kind: "success" | "error" | "info", message: string) => void;
}) {
  const { t } = useI18n();
  const headers = useMemo(() => authHeaders(sessionToken), [sessionToken]);
  const {
    providerRows,
    selectedProvider,
    providerForm,
    setProviderForm,
    credentialRows,
    statusRows,
    selectProvider,
    beginCreateProvider,
    loadProviders,
    loadProviderScopedData,
    reloadAndReselect,
  } = useProviderData(sessionToken);
  const [activeTab, setActiveTab] = useState<ProviderWorkspaceTab>("config");
  const [credentialForm, setCredentialForm] = useState<CredentialFormState>({
    values: emptyCredentialValuesForChannel(providerForm.channel),
    editingIndex: null,
    rawJson: "",
  });
  const [oauthFlow, setOauthFlow] = useState<OAuthStartResponse | null>(null);
  const [oauthCallbackUrl, setOauthCallbackUrl] = useState("");
  const [oauthCallbackResult, setOauthCallbackResult] = useState<OAuthCallbackResponse | null>(null);
  const [usageByCredential, setUsageByCredential] = useState<Record<number, string>>({});
  const [usageRowsByCredential, setUsageRowsByCredential] = useState<Record<number, LiveUsageRow[]>>({});
  const [usageLoadingByCredential, setUsageLoadingByCredential] = useState<Record<number, boolean>>({});
  const [allModelRows, setAllModelRows] = useState<MemoryModelRow[]>([]);
  const [selectedModelId, setSelectedModelId] = useState<number | null>(null);
  const [modelForm, setModelForm] = useState<ModelFormState>({
    id: "",
    model_id: "",
    display_name: "",
    enabled: true,
    pricing_json: "",
    alias_of: "",
  });

  useEffect(() => {
    setCredentialForm({
      values: emptyCredentialValuesForChannel(providerForm.channel),
      editingIndex: null,
      rawJson: "",
    });
  }, [providerForm.channel, selectedProvider?.id]);

  useEffect(() => {
    setOauthFlow(null);
    setOauthCallbackUrl("");
    setOauthCallbackResult(null);
    setUsageByCredential({});
    setUsageRowsByCredential({});
    setUsageLoadingByCredential({});
    setSelectedModelId(null);
  }, [selectedProvider?.id, providerForm.channel]);

  const providerModelRows = useMemo(
    () => filterModelsForProvider(allModelRows, selectedProvider?.id ?? null),
    [allModelRows, selectedProvider?.id],
  );

  const beginCreateModel = () => {
    setSelectedModelId(null);
    setModelForm({
      id: nextResourceId(allModelRows),
      model_id: "",
      display_name: "",
      enabled: true,
      pricing_json: "",
      alias_of: "",
    });
  };

  const channelOptions = useMemo(
    () =>
      [
        "openai",
        "anthropic",
        "aistudio",
        "vertex",
        "vertexexpress",
        "geminicli",
        "antigravity",
        "claudecode",
        "codex",
        "nvidia",
        "deepseek",
        "groq",
        "openrouter",
        "custom",
      ].map((value) => ({ value, label: value })),
    [],
  );

  const updateProviderForm = (patch: Partial<typeof providerForm>) => {
    const nextChannel = patch.channel ?? providerForm.channel;
    setProviderForm((current) => ({
      ...current,
      ...patch,
      settings:
        patch.channel && patch.channel !== current.channel
          ? defaultSettingsForChannel(nextChannel)
          : patch.settings ?? current.settings,
      dispatchRules:
        patch.channel && patch.channel !== current.channel
          ? [createDispatchRuleDraft()]
          : patch.dispatchRules ?? current.dispatchRules,
    }));
  };

  const loadDefaultDispatch = async (channel: string) => {
    const document = await apiJson<DispatchTableDocument>("/admin/providers/default-dispatch", {
      method: "POST",
      headers,
      body: JSON.stringify({ channel } satisfies ProviderDispatchTemplateParams),
    });
    return dispatchDraftsFromDocument(document);
  };

  useEffect(() => {
    if (selectedProvider) {
      return;
    }
    let active = true;
    const channel = providerForm.channel;
    const formId = providerForm.id;
    void loadDefaultDispatch(channel)
      .then((dispatchRules) => {
        if (!active) {
          return;
        }
        setProviderForm((current) =>
          current.id === formId && current.channel === channel
            ? { ...current, dispatchRules }
            : current,
        );
      })
      .catch((error) => {
        if (!active) {
          return;
        }
        notify("error", error instanceof Error ? error.message : String(error));
      });
    return () => {
      active = false;
    };
  }, [headers, notify, providerForm.channel, providerForm.id, selectedProvider]);

  useEffect(() => {
    if (!selectedProvider) {
      setAllModelRows([]);
      beginCreateModel();
      return;
    }
    let active = true;
    void apiJson<MemoryModelRow[]>("/admin/models/query", {
      method: "POST",
      headers,
      body: JSON.stringify({}),
    })
      .then((models) => {
        if (!active) {
          return;
        }
        setAllModelRows(models);
      })
      .catch((error) => {
        if (!active) {
          return;
        }
        notify("error", error instanceof Error ? error.message : String(error));
      });
    return () => {
      active = false;
    };
  }, [headers, notify, selectedProvider?.id]);

  useEffect(() => {
    if (!selectedProvider) {
      return;
    }
    if (selectedModelId === null) {
      beginCreateModel();
    }
  }, [allModelRows, selectedModelId, selectedProvider?.id]);

  const saveProvider = async () => {
    try {
      const payload: ProviderWrite = {
        id: parseRequiredI64(providerForm.id, "id"),
        name: providerForm.name.trim(),
        channel: providerForm.channel.trim(),
        settings_json: JSON.stringify(
          buildChannelSettingsJson(providerForm.channel, providerForm.settings),
        ),
        dispatch_json: JSON.stringify(buildDispatchDocument(providerForm.dispatchRules)),
      };
      await apiJson("/admin/providers/upsert", {
        method: "POST",
        headers,
        body: JSON.stringify(payload),
      });
      notify("success", t("providers.saved"));
      await reloadAndReselect(payload.name);
    } catch (error) {
      notify("error", error instanceof Error ? error.message : String(error));
    }
  };

  const deleteProvider = async () => {
    if (!selectedProvider) {
      return;
    }
    try {
      await apiJson("/admin/providers/delete", {
        method: "POST",
        headers,
        body: JSON.stringify({ name: selectedProvider.name }),
      });
      notify("success", t("providers.deleted"));
      beginCreateProvider();
      await loadProviders();
    } catch (error) {
      notify("error", error instanceof Error ? error.message : String(error));
    }
  };

  const editCredential = (row: CredentialRow) => {
    setCredentialForm({
      editingIndex: row.index,
      values: credentialValuesFromJson(selectedProvider?.channel ?? "custom", row.credential),
      rawJson: "",
    });
  };

  const saveCredential = async () => {
    if (!selectedProvider) {
      notify("error", t("providers.error.needProvider"));
      return;
    }
    try {
      let credential: Record<string, unknown>;
      if (credentialForm.editingIndex === null && credentialForm.rawJson.trim()) {
        const raw = credentialForm.rawJson.trim();
        if (raw.startsWith("{")) {
          credential = JSON.parse(raw);
        } else {
          // Plain string — wrap as cookie for claudecode/codex, api_key for others
          const channel = selectedProvider.channel;
          if (channel === "claudecode" || channel === "codex") {
            credential = { cookie: raw };
          } else {
            credential = { api_key: raw };
          }
        }
      } else {
        credential = buildCredentialJson(selectedProvider.channel, credentialForm.values);
      }
      if (credentialForm.editingIndex !== null) {
        await apiVoid("/admin/credentials/delete", {
          method: "POST",
          headers,
          body: JSON.stringify({
            provider_name: selectedProvider.name,
            index: credentialForm.editingIndex,
          }),
        });
      }
      await apiJson("/admin/credentials/upsert", {
        method: "POST",
        headers,
        body: JSON.stringify({
          provider_name: selectedProvider.name,
          credential,
        }),
      });
      notify("success", t("providers.credentials.saved"));
      await loadProviderScopedData(selectedProvider);
      setCredentialForm({
        editingIndex: null,
        values: emptyCredentialValuesForChannel(selectedProvider.channel),
        rawJson: "",
      });
    } catch (error) {
      notify("error", error instanceof Error ? error.message : String(error));
    }
  };

  const deleteCredential = async (row: CredentialRow) => {
    try {
      await apiVoid("/admin/credentials/delete", {
        method: "POST",
        headers,
        body: JSON.stringify({
          provider_name: row.provider,
          index: row.index,
        }),
      });
      notify("success", t("providers.credentials.deleted"));
      if (selectedProvider) {
        await loadProviderScopedData(selectedProvider);
      }
    } catch (error) {
      notify("error", error instanceof Error ? error.message : String(error));
    }
  };

  const saveModel = async () => {
    if (!selectedProvider) {
      notify("error", t("providers.error.needProvider"));
      return;
    }
    try {
      const aliasOf = modelForm.alias_of.trim() ? Number(modelForm.alias_of) : null;
      // Validate pricing JSON before sending — catches user typos before the
      // round-trip and keeps error messages local.
      let pricing_json: string | null = null;
      const trimmed = modelForm.pricing_json.trim();
      if (trimmed) {
        try {
          JSON.parse(trimmed);
        } catch (e) {
          notify(
            "error",
            `Invalid pricing JSON: ${e instanceof Error ? e.message : String(e)}`,
          );
          return;
        }
        pricing_json = trimmed;
      }
      const payload: ModelWrite = {
        id: parseRequiredI64(modelForm.id, "id"),
        provider_id: selectedProvider.id,
        model_id: modelForm.model_id.trim(),
        display_name: modelForm.display_name.trim() || null,
        enabled: modelForm.enabled,
        price_each_call: null,
        price_tiers_json: null,
        pricing_json,
        alias_of: aliasOf,
      };
      await apiJson("/admin/models/upsert", {
        method: "POST",
        headers,
        body: JSON.stringify(payload),
      });
      notify("success", t("models.saved"));
      const rows = await apiJson<MemoryModelRow[]>("/admin/models/query", {
        method: "POST",
        headers,
        body: JSON.stringify({}),
      });
      setAllModelRows(rows);
      setSelectedModelId(payload.id);
    } catch (error) {
      notify("error", error instanceof Error ? error.message : String(error));
    }
  };

  const deleteModel = async (id: number) => {
    try {
      await apiVoid("/admin/models/delete", {
        method: "POST",
        headers,
        body: JSON.stringify({ id }),
      });
      notify("success", t("models.deleted"));
      const rows = await apiJson<MemoryModelRow[]>("/admin/models/query", {
        method: "POST",
        headers,
        body: JSON.stringify({}),
      });
      setAllModelRows(rows);
      beginCreateModel();
    } catch (error) {
      notify("error", error instanceof Error ? error.message : String(error));
    }
  };

  const modelsBatch = useBatchSelection<MemoryModelRow, number>({
    rows: providerModelRows,
    getKey: (row) => row.id,
    onBatchDelete: async (ids) => {
      await apiVoid("/admin/models/batch-delete", {
        method: "POST",
        headers,
        body: JSON.stringify(ids),
      });
    },
    onSuccess: (count) => {
      notify("success", t("batch.deleted", { count }));
      if (selectedModelId != null && modelsBatch.selectedKeys.has(selectedModelId)) {
        beginCreateModel();
      }
      void reloadModels();
    },
    onError: (err) => {
      notify("error", err instanceof Error ? err.message : String(err));
    },
    confirmMessage: (count) => t("batch.confirm", { count }),
  });

  const reloadModels = async () => {
    const models = await apiJson<MemoryModelRow[]>("/admin/models/query", {
      method: "POST",
      headers,
      body: JSON.stringify({}),
    });
    setAllModelRows(models);
  };

  const pullModels = async (): Promise<string[]> => {
    if (!selectedProvider) return [];
    const resp = await apiJson<{ models: string[] }>("/admin/models/pull", {
      method: "POST",
      headers,
      body: JSON.stringify({ provider_id: selectedProvider.id }),
    });
    return resp.models;
  };

  const importPulledModels = async (models: string[]) => {
    if (!selectedProvider || models.length === 0) return;
    try {
      const maxId = allModelRows.reduce((max, row) => Math.max(max, row.id), 0);
      const items: ModelWrite[] = models.map((model, index) => ({
        id: maxId + index + 1,
        provider_id: selectedProvider.id,
        model_id: model,
        display_name: null,
        enabled: true,
        price_each_call: null,
        price_tiers_json: null,
        pricing_json: null,
        alias_of: null,
      }));
      await apiJson("/admin/models/batch-upsert", {
        method: "POST",
        headers,
        body: JSON.stringify(items),
      });
      notify("success", t("models.pull.imported", { count: items.length }));
      await reloadModels();
    } catch (error) {
      notify("error", error instanceof Error ? error.message : String(error));
    }
  };

  /// Create an alias row for a suffix variant (model_id + suffix pointing at
  /// the base model) and append matching rewrite rules to the provider's
  /// settings_json, all scoped to the new alias name via model_pattern.
  const addSuffixVariant = async (
    base: MemoryModelRow,
    suffix: string,
    actions: Array<{ path: string; value: unknown }>,
  ) => {
    if (!selectedProvider) return;
    const aliasName = `${base.model_id}${suffix}`;
    try {
      // 1. Create the alias row (check duplicate first).
      const existing = allModelRows.find(
        (m) =>
          m.provider_id === selectedProvider.id &&
          m.model_id === aliasName &&
          m.alias_of != null,
      );
      if (!existing) {
        const maxId = allModelRows.reduce((max, row) => Math.max(max, row.id), 0);
        const aliasPayload: ModelWrite = {
          id: maxId + 1,
          provider_id: selectedProvider.id,
          model_id: aliasName,
          display_name: null,
          enabled: true,
          price_each_call: null,
          price_tiers_json: null,
          pricing_json: null,
          alias_of: base.id,
        };
        await apiJson("/admin/models/upsert", {
          method: "POST",
          headers,
          body: JSON.stringify(aliasPayload),
        });
      }

      // 2. Append rewrite rules to the provider's settings_json, scoped by
      // model_pattern to the new alias name.
      const existingRulesRaw = providerForm.settings.rewrite_rules ?? "[]";
      let existingRules: unknown[] = [];
      try {
        const parsed = JSON.parse(existingRulesRaw);
        if (Array.isArray(parsed)) existingRules = parsed;
      } catch {
        existingRules = [];
      }
      const newRules = actions.map((a) => ({
        path: a.path,
        action: { type: "Set", value: a.value },
        filter: { model_pattern: aliasName },
      }));
      const mergedRulesJson = JSON.stringify([...existingRules, ...newRules]);

      // Update provider with the merged rewrite_rules.
      const payload: ProviderWrite = {
        id: parseRequiredI64(providerForm.id, "id"),
        name: providerForm.name.trim(),
        channel: providerForm.channel.trim(),
        settings_json: JSON.stringify(
          buildChannelSettingsJson(providerForm.channel, {
            ...providerForm.settings,
            rewrite_rules: mergedRulesJson,
          }),
        ),
        dispatch_json: JSON.stringify(buildDispatchDocument(providerForm.dispatchRules)),
      };
      await apiJson("/admin/providers/upsert", {
        method: "POST",
        headers,
        body: JSON.stringify(payload),
      });

      // Reflect the new rewrite rules in local form state.
      updateProviderForm({
        settings: {
          ...providerForm.settings,
          rewrite_rules: mergedRulesJson,
        },
      });

      notify("success", t("models.suffixDialog.created", { name: aliasName }));
      await reloadModels();
    } catch (error) {
      notify("error", error instanceof Error ? error.message : String(error));
    }
  };

  const updateStatus = async (
    row: { provider: string; index: number },
    status: "healthy" | "dead",
  ) => {
    try {
      await apiJson("/admin/credential-statuses/update", {
        method: "POST",
        headers,
        body: JSON.stringify({
          provider_name: row.provider,
          index: row.index,
          status,
        }),
      });
      notify("success", t("providers.status.updated"));
      if (selectedProvider) {
        await loadProviderScopedData(selectedProvider);
      }
    } catch (error) {
      notify("error", error instanceof Error ? error.message : String(error));
    }
  };

  const loadOAuthStart = async () => {
    if (!selectedProvider) {
      notify("error", t("providers.error.needProvider"));
      return;
    }
    try {
      const result = await apiJson<OAuthStartResponse>(
        `/${encodeURIComponent(selectedProvider.name)}/v1/oauth`,
        { headers: authHeaders(sessionToken, false) },
      );
      setOauthFlow(result);
      notify("info", t("providers.oauth.started"));
      window.open(result.authorize_url, "_blank", "noopener,noreferrer");
    } catch (error) {
      notify("error", error instanceof Error ? error.message : String(error));
    }
  };

  const loadOAuthFinish = async () => {
    if (!selectedProvider) {
      notify("error", t("providers.error.needProvider"));
      return;
    }
    try {
      const query = buildOAuthCallbackQuery(oauthCallbackUrl);
      const result = await apiJson<OAuthCallbackResponse>(
        `/${encodeURIComponent(selectedProvider.name)}/v1/oauth/callback${query}`,
        { headers: authHeaders(sessionToken, false) },
      );
      setOauthCallbackResult(result);
      notify("info", t("providers.oauth.finished"));
      await loadProviderScopedData(selectedProvider);
      setActiveTab("credentials");
    } catch (error) {
      notify("error", error instanceof Error ? error.message : String(error));
    }
  };

  const loadUsage = async (row: CredentialRow) => {
    if (!selectedProvider) {
      notify("error", t("providers.error.needProvider"));
      return;
    }
    try {
      setUsageLoadingByCredential((current) => ({ ...current, [row.index]: true }));
      const payload = await apiJson<unknown>(
        `/${encodeURIComponent(selectedProvider.name)}/v1/usage?credential_index=${encodeURIComponent(String(row.index))}`,
        { headers: authHeaders(sessionToken, false) },
      );
      const raw = typeof payload === "string" ? payload : JSON.stringify(payload ?? {}, null, 2);
      setUsageByCredential((current) => ({ ...current, [row.index]: raw }));
      setUsageRowsByCredential((current) => ({
        ...current,
        [row.index]: parseLiveUsageRows(selectedProvider.channel, payload),
      }));
      notify("info", t("providers.usage.loaded"));
    } catch (error) {
      notify("error", error instanceof Error ? error.message : String(error));
    } finally {
      setUsageLoadingByCredential((current) => ({ ...current, [row.index]: false }));
    }
  };

  return (
    <div className="space-y-4">
      <div className="grid gap-4 lg:grid-cols-[320px_minmax(0,1fr)]">
        <ProviderList
          rows={providerRows}
          selectedProviderId={selectedProvider?.id ?? null}
          onSelect={(row) => void selectProvider(row)}
          onCreate={() => {
            beginCreateProvider();
            setActiveTab("config");
          }}
          onRefresh={() => void loadProviders()}
          title={t("providers.title")}
          emptyLabel={t("providers.empty")}
          newLabel={t("providers.new")}
          refreshLabel={t("providers.refresh")}
        />
        <div className="space-y-4">
          <div className="flex flex-wrap gap-2">
            {(["config", "models", "rewrite", "credentials", "oauth"] as ProviderWorkspaceTab[]).map(
              (tab) => (
                <Button
                  key={tab}
                  variant={activeTab === tab ? "primary" : "neutral"}
                  onClick={() => setActiveTab(tab)}
                >
                  {tab === "models" ? t("models.title") : t(`providers.tab.${tab}`)}
                </Button>
              ),
            )}
          </div>
          {activeTab === "config" ? (
            <ConfigTab
              form={providerForm}
              onChange={updateProviderForm}
              onSave={() => void saveProvider()}
              onDelete={() => void deleteProvider()}
              channelOptions={channelOptions}
              canDelete={Boolean(selectedProvider)}
              labels={{
                subtitle: t("providers.subtitle"),
                name: t("providers.form.name"),
                channel: t("providers.form.channel"),
                dispatchRules: t("providers.form.dispatchRules"),
                dispatchHint: t("providers.form.dispatchHint"),
                dispatchRule: t("providers.dispatch.rule"),
                dispatchSourceOperation: t("providers.dispatch.sourceOperation"),
                dispatchSourceProtocol: t("providers.dispatch.sourceProtocol"),
                dispatchMode: t("providers.dispatch.mode"),
                dispatchDestinationOperation: t("providers.dispatch.destinationOperation"),
                dispatchDestinationProtocol: t("providers.dispatch.destinationProtocol"),
                dispatchAddRule: t("providers.dispatch.addRule"),
                dispatchRemoveRule: t("providers.dispatch.removeRule"),
                dispatchExpand: t("providers.dispatch.expand"),
                dispatchCollapse: t("providers.dispatch.collapse"),
                dispatchCollapsedSummary: t("providers.dispatch.collapsedSummary"),
                modePassthrough: t("providers.dispatch.mode.passthrough"),
                modeTransformTo: t("providers.dispatch.mode.transformTo"),
                modeLocal: t("providers.dispatch.mode.local"),
                modeUnsupported: t("providers.dispatch.mode.unsupported"),
                save: t("providers.form.save"),
                delete: t("providers.form.delete"),
                newHint: t("providers.form.newHint"),
              }}
            />
          ) : null}
          {activeTab === "credentials" ? (
            <CredentialsTab
              channel={selectedProvider?.channel ?? providerForm.channel}
              credentials={credentialRows}
              form={credentialForm}
              onChangeForm={setCredentialForm}
              onEdit={editCredential}
              onNew={() => setCredentialForm({ values: emptyCredentialValuesForChannel(selectedProvider?.channel ?? providerForm.channel), editingIndex: null, rawJson: "" })}
              onDelete={(row) => void deleteCredential(row)}
              onSave={() => void saveCredential()}
              statuses={statusRows}
              onUpdateStatus={(row, status) => void updateStatus(row, status)}
              supportsUsage={supportsCredentialUsageChannel(selectedProvider?.channel ?? providerForm.channel)}
              usageByCredential={usageByCredential}
              usageRowsByCredential={usageRowsByCredential}
              usageLoadingByCredential={usageLoadingByCredential}
              onQueryUsage={(row) => void loadUsage(row)}
              labels={{
                title: t("providers.tab.credentials"),
                add: t("providers.credentials.add"),
                replace: t("providers.credentials.replace"),
                importJsonPlaceholder: t("providers.credentials.importJsonPlaceholder"),
                none: t("providers.credentials.none"),
                edit: t("providers.credentials.edit"),
                delete: t("providers.credentials.delete"),
                showJson: t("providers.credentials.showJson"),
                hideJson: t("providers.credentials.hideJson"),
                expandJson: t("providers.credentials.showJson"),
                collapseJson: t("providers.credentials.hideJson"),
                configured: t("providers.credentials.configured"),
                statusNone: t("providers.status.none"),
                statusHealthy: t("providers.status.healthy"),
                statusCooldown: t("providers.status.cooldown"),
                statusDead: t("providers.status.dead"),
                usageFetch: t("providers.usage.fetch"),
                usageTitle: t("providers.usage.title"),
                usageShow: t("providers.usage.show"),
                usageHide: t("providers.usage.hide"),
                usageLimit: t("providers.usage.limit"),
                usagePercent: t("providers.usage.percent"),
                usageReset: t("providers.usage.reset"),
                usageRaw: t("providers.usage.raw"),
                usageEmpty: t("providers.usage.emptyState"),
                loading: t("common.loading"),
              }}
            />
          ) : null}
          {activeTab === "models" ? (
            <ModelsTab
              rows={providerModelRows}
              selectedId={selectedModelId}
              form={modelForm}
              onSelect={(row) => {
                setSelectedModelId(row.id);
                setModelForm({
                  id: String(row.id),
                  model_id: row.model_id,
                  display_name: row.display_name ?? "",
                  enabled: row.enabled,
                  pricing_json: row.pricing_json
                    ? (() => {
                        try {
                          return JSON.stringify(JSON.parse(row.pricing_json), null, 2);
                        } catch {
                          return row.pricing_json;
                        }
                      })()
                    : "",
                  alias_of: row.alias_of != null ? String(row.alias_of) : "",
                });
              }}
              onCreate={beginCreateModel}
              onChangeForm={(patch) => setModelForm((current) => ({ ...current, ...patch }))}
              onSave={() => void saveModel()}
              onDelete={(id) => void deleteModel(id)}
              onPull={pullModels}
              onImport={(models) => void importPulledModels(models)}
              onAddSuffixVariant={(base, suffix, actions) =>
                void addSuffixVariant(base, suffix, actions)
              }
              providerChannel={providerForm.channel}
              labels={{
                title: t("models.title"),
                empty: t("common.noData"),
                create: t("common.create"),
                save: t("common.save"),
                delete: t("common.delete"),
                cancel: t("common.cancel"),
                modelId: t("common.modelId"),
                displayName: t("common.displayName"),
                enabled: t("common.enabled"),
                pricingJson: t("models.pricingJson"),
                pricingJsonHint: t("models.pricingJson.hint"),
                aliasOf: t("models.aliasOf"),
                aliasOfNone: t("models.aliasOf.none"),
                aliasBadge: t("models.aliasBadge"),
                filterAll: t("models.filter.all"),
                filterReal: t("models.filter.real"),
                filterAliases: t("models.filter.aliases"),
                priceOverrideHint: t("models.priceOverrideHint"),
                pull: t("models.pull"),
                pullLoading: t("models.pull.loading"),
                pullEmpty: t("models.pull.empty"),
                pullFound: t("models.pull.found"),
                pullImport: t("models.pull.importSelected"),
                pullSelectAll: t("models.pull.selectAll"),
                pullDeselectAll: t("models.pull.deselectAll"),
                addSuffixVariant: t("models.suffixVariant"),
                suffixDialogTitle: t("models.suffixDialog.title"),
                suffixDialogHint: t("models.suffixDialog.hint"),
                suffixProtocol: t("models.suffixDialog.protocol"),
                suffixNone: t("models.suffixDialog.none"),
                suffixPreview: t("models.suffixDialog.preview"),
                suffixConfirm: t("models.suffixDialog.confirm"),
                pricingEditor: {
                  modeStructured: t("models.pricing.mode.structured"),
                  modeJson: t("models.pricing.mode.json"),
                  priceEachCall: t("models.pricing.priceEachCall"),
                  priceTiers: t("models.pricing.priceTiers"),
                  flexPriceEachCall: t("models.pricing.flexPriceEachCall"),
                  flexPriceTiers: t("models.pricing.flexPriceTiers"),
                  scalePriceEachCall: t("models.pricing.scalePriceEachCall"),
                  scalePriceTiers: t("models.pricing.scalePriceTiers"),
                  priorityPriceEachCall: t("models.pricing.priorityPriceEachCall"),
                  priorityPriceTiers: t("models.pricing.priorityPriceTiers"),
                  addTier: t("models.pricing.addTier"),
                  removeRow: t("models.pricing.removeRow"),
                  tierInputTokensUpTo: t("models.pricing.tier.inputTokensUpTo"),
                  tierPriceInput: t("models.pricing.tier.priceInput"),
                  tierPriceOutput: t("models.pricing.tier.priceOutput"),
                  tierPriceCacheRead: t("models.pricing.tier.priceCacheRead"),
                  tierPriceCacheCreation: t("models.pricing.tier.priceCacheCreation"),
                  tierPriceCacheCreation5min: t("models.pricing.tier.priceCacheCreation5min"),
                  tierPriceCacheCreation1h: t("models.pricing.tier.priceCacheCreation1h"),
                  emptyHint: t("models.pricing.emptyHint"),
                  jsonParseError: t("models.pricing.jsonParseError"),
                  jsonTextareaLabel: t("models.pricing.jsonTextareaLabel"),
                },
              }}
              batch={{
                batchMode: modelsBatch.batchMode,
                selectedCount: modelsBatch.selectedCount,
                pending: modelsBatch.pending,
                isSelected: modelsBatch.isSelected,
                onEnter: modelsBatch.enterBatch,
                onExit: modelsBatch.exitBatch,
                onSelectAll: modelsBatch.selectAll,
                onClear: modelsBatch.clear,
                onDelete: () => void modelsBatch.deleteSelected(),
                onToggleRow: modelsBatch.toggle,
              }}
            />
          ) : null}
          {activeTab === "rewrite" ? (
            <RewriteRulesTab
              form={providerForm}
              onChange={updateProviderForm}
              onSave={() => void saveProvider()}
              modelNames={providerModelRows.map((r) => r.model_id)}
              notify={notify}
            />
          ) : null}
          {activeTab === "oauth" ? (
            <OAuthTab
              flow={oauthFlow}
              callbackUrl={oauthCallbackUrl}
              callbackResult={oauthCallbackResult}
              onChangeCallbackUrl={setOauthCallbackUrl}
              onStart={() => void loadOAuthStart()}
              onOpenAuthorize={() => {
                if (oauthFlow?.authorize_url) {
                  window.open(oauthFlow.authorize_url, "_blank", "noopener,noreferrer");
                }
              }}
              onFinish={() => void loadOAuthFinish()}
              labels={{
                start: t("providers.oauth.start"),
                finish: t("providers.oauth.finish"),
                startHint: t("providers.oauth.startHint"),
                openAuthorize: t("providers.oauth.openAuthorize"),
                redirectUri: t("providers.oauth.redirectUri"),
                callbackUrl: t("providers.oauth.callbackUrl"),
                callbackHint: t("providers.oauth.callbackHint"),
                persistedCredential: t("providers.oauth.persistedCredential"),
              }}
            />
          ) : null}
        </div>
      </div>
    </div>
  );
}
