import { useEffect, useMemo, useState } from "react";

import { useI18n } from "../../app/i18n";
import { Button } from "../../components/ui";
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
import { ModelAliasesTab } from "./providers/ModelAliasesTab";
import { ModelsTab } from "./providers/ModelsTab";
import { OAuthTab } from "./providers/OAuthTab";
import { ProviderList } from "./providers/ProviderList";
import {
  filterAliasesForProvider,
  filterModelsForProvider,
  nextResourceId,
} from "./providers/resources";
import { useProviderData } from "./providers/hooks/useProviderData";
import type { CredentialFormState, ProviderWorkspaceTab } from "./providers";
import { parseLiveUsageRows, supportsCredentialUsageChannel, type LiveUsageRow } from "./providers/usage";
import type { MemoryModelAliasRow, MemoryModelRow, ModelAliasWrite, ModelWrite } from "../../lib/types/admin";

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
  const [allAliasRows, setAllAliasRows] = useState<MemoryModelAliasRow[]>([]);
  const [selectedModelId, setSelectedModelId] = useState<number | null>(null);
  const [selectedAliasId, setSelectedAliasId] = useState<number | null>(null);
  const [modelForm, setModelForm] = useState({
    id: "",
    model_id: "",
    display_name: "",
    enabled: true,
    price_each_call: "",
    price_tiers_json: "[]",
  });
  const [aliasForm, setAliasForm] = useState({
    id: "",
    alias: "",
    model_id: "",
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
    setSelectedAliasId(null);
  }, [selectedProvider?.id, providerForm.channel]);

  const providerModelRows = useMemo(
    () => filterModelsForProvider(allModelRows, selectedProvider?.id ?? null),
    [allModelRows, selectedProvider?.id],
  );
  const providerAliasRows = useMemo(
    () => filterAliasesForProvider(allAliasRows, selectedProvider?.name ?? null),
    [allAliasRows, selectedProvider?.name],
  );

  const beginCreateModel = () => {
    setSelectedModelId(null);
    setModelForm({
      id: nextResourceId(allModelRows),
      model_id: "",
      display_name: "",
      enabled: true,
      price_each_call: "",
      price_tiers_json: "[]",
    });
  };

  const beginCreateAlias = () => {
    setSelectedAliasId(null);
    setAliasForm({
      id: nextResourceId(allAliasRows),
      alias: "",
      model_id: "",
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
      setAllAliasRows([]);
      beginCreateModel();
      beginCreateAlias();
      return;
    }
    let active = true;
    void Promise.all([
      apiJson<MemoryModelRow[]>("/admin/models/query", {
        method: "POST",
        headers,
        body: JSON.stringify({}),
      }),
      apiJson<MemoryModelAliasRow[]>("/admin/model-aliases/query", {
        method: "POST",
        headers,
        body: JSON.stringify({}),
      }),
    ])
      .then(([models, aliases]) => {
        if (!active) {
          return;
        }
        setAllModelRows(models);
        setAllAliasRows(aliases);
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

  useEffect(() => {
    if (!selectedProvider) {
      return;
    }
    if (selectedAliasId === null) {
      beginCreateAlias();
    }
  }, [allAliasRows, selectedAliasId, selectedProvider?.id]);

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
      const payload: ModelWrite = {
        id: parseRequiredI64(modelForm.id, "id"),
        provider_id: selectedProvider.id,
        model_id: modelForm.model_id.trim(),
        display_name: modelForm.display_name.trim() || null,
        enabled: modelForm.enabled,
        price_each_call: modelForm.price_each_call.trim() ? Number(modelForm.price_each_call) : null,
        price_tiers_json: modelForm.price_tiers_json,
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

  const saveAlias = async () => {
    if (!selectedProvider) {
      notify("error", t("providers.error.needProvider"));
      return;
    }
    try {
      const payload: ModelAliasWrite = {
        id: parseRequiredI64(aliasForm.id, "id"),
        alias: aliasForm.alias.trim(),
        provider_id: selectedProvider.id,
        model_id: aliasForm.model_id.trim(),
        enabled: true,
      };
      await apiJson("/admin/model-aliases/upsert", {
        method: "POST",
        headers,
        body: JSON.stringify(payload),
      });
      notify("success", t("modelAliases.saved"));
      const rows = await apiJson<MemoryModelAliasRow[]>("/admin/model-aliases/query", {
        method: "POST",
        headers,
        body: JSON.stringify({}),
      });
      setAllAliasRows(rows);
      setSelectedAliasId(payload.id);
    } catch (error) {
      notify("error", error instanceof Error ? error.message : String(error));
    }
  };

  const deleteAlias = async (alias: string) => {
    try {
      await apiVoid("/admin/model-aliases/delete", {
        method: "POST",
        headers,
        body: JSON.stringify({ alias }),
      });
      notify("success", t("modelAliases.deleted"));
      const rows = await apiJson<MemoryModelAliasRow[]>("/admin/model-aliases/query", {
        method: "POST",
        headers,
        body: JSON.stringify({}),
      });
      setAllAliasRows(rows);
      beginCreateAlias();
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
      <div className="grid gap-4 xl:grid-cols-[360px_minmax(0,1fr)]">
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
            {(["config", "models", "aliases", "credentials", "oauth"] as ProviderWorkspaceTab[]).map(
              (tab) => (
                <Button
                  key={tab}
                  variant={activeTab === tab ? "primary" : "neutral"}
                  onClick={() => setActiveTab(tab)}
                >
                  {tab === "models"
                    ? t("models.title")
                    : tab === "aliases"
                      ? t("modelAliases.title")
                      : t(`providers.tab.${tab}`)}
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
                  price_each_call: row.price_each_call?.toString() ?? "",
                  price_tiers_json: JSON.stringify(row.price_tiers, null, 2),
                });
              }}
              onCreate={beginCreateModel}
              onChangeForm={(patch) => setModelForm((current) => ({ ...current, ...patch }))}
              onSave={() => void saveModel()}
              onDelete={(id) => void deleteModel(id)}
              labels={{
                title: t("models.title"),
                empty: t("common.noData"),
                create: t("common.create"),
                save: t("common.save"),
                delete: t("common.delete"),
                modelId: t("common.modelId"),
                displayName: t("common.displayName"),
                enabled: t("common.enabled"),
                priceEachCall: t("common.priceEachCall"),
                priceTiersJson: t("common.priceTiersJson"),
              }}
            />
          ) : null}
          {activeTab === "aliases" ? (
            <ModelAliasesTab
              rows={providerAliasRows}
              selectedId={selectedAliasId}
              form={aliasForm}
              onSelect={(row) => {
                setSelectedAliasId(row.id);
                setAliasForm({
                  id: String(row.id),
                  alias: row.alias,
                  model_id: row.model_id,
                });
              }}
              onCreate={beginCreateAlias}
              onChangeForm={(patch) => setAliasForm((current) => ({ ...current, ...patch }))}
              onSave={() => void saveAlias()}
              onDelete={(alias) => void deleteAlias(alias)}
              labels={{
                title: t("modelAliases.title"),
                empty: t("common.noData"),
                create: t("common.create"),
                save: t("common.save"),
                delete: t("common.delete"),
                alias: "alias",
                modelId: t("common.modelId"),
              }}
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
