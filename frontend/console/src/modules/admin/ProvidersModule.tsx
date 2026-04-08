import { useEffect, useMemo, useState } from "react";

import { useI18n } from "../../app/i18n";
import { Button } from "../../components/ui";
import { apiJson, apiText, apiVoid } from "../../lib/api";
import { authHeaders } from "../../lib/auth";
import { parseRequiredI64 } from "../../lib/form";
import type {
  CredentialHealthRow,
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
import { OAuthTab } from "./providers/OAuthTab";
import { ProviderList } from "./providers/ProviderList";
import { StatusTab } from "./providers/StatusTab";
import { UsageTab } from "./providers/UsageTab";
import { useProviderData } from "./providers/hooks/useProviderData";
import type { CredentialFormState, ProviderWorkspaceTab } from "./providers";

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
  });
  const [oauthFlow, setOauthFlow] = useState<OAuthStartResponse | null>(null);
  const [oauthCallbackUrl, setOauthCallbackUrl] = useState("");
  const [oauthCallbackResult, setOauthCallbackResult] = useState<OAuthCallbackResponse | null>(null);
  const [usageResult, setUsageResult] = useState("");

  useEffect(() => {
    setCredentialForm({
      values: emptyCredentialValuesForChannel(providerForm.channel),
      editingIndex: null,
    });
  }, [providerForm.channel, selectedProvider?.id]);

  useEffect(() => {
    setOauthFlow(null);
    setOauthCallbackUrl("");
    setOauthCallbackResult(null);
  }, [selectedProvider?.id, providerForm.channel]);

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
    });
  };

  const saveCredential = async () => {
    if (!selectedProvider) {
      notify("error", t("providers.error.needProvider"));
      return;
    }
    try {
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
          credential: buildCredentialJson(selectedProvider.channel, credentialForm.values),
        }),
      });
      notify("success", t("providers.credentials.saved"));
      await loadProviderScopedData(selectedProvider);
      setCredentialForm({
        editingIndex: null,
        values: emptyCredentialValuesForChannel(selectedProvider.channel),
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

  const updateStatus = async (row: CredentialHealthRow, status: "healthy" | "dead") => {
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

  const loadUsage = async () => {
    if (!selectedProvider) {
      notify("error", t("providers.error.needProvider"));
      return;
    }
    try {
      const result = await apiText(
        `/${encodeURIComponent(selectedProvider.name)}/v1/usage`,
        { headers: authHeaders(sessionToken, false) },
      );
      setUsageResult(result);
      notify("info", t("providers.usage.loaded"));
    } catch (error) {
      notify("error", error instanceof Error ? error.message : String(error));
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
            {(["config", "credentials", "status", "oauth", "usage"] as ProviderWorkspaceTab[]).map(
              (tab) => (
                <Button
                  key={tab}
                  variant={activeTab === tab ? "primary" : "neutral"}
                  onClick={() => setActiveTab(tab)}
                >
                  {t(`providers.tab.${tab}`)}
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
                id: t("providers.form.id"),
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
              onDelete={(row) => void deleteCredential(row)}
              onSave={() => void saveCredential()}
              labels={{
                title: t("providers.tab.credentials"),
                add: t("providers.credentials.add"),
                replace: t("providers.credentials.replace"),
                none: t("providers.credentials.none"),
                edit: t("providers.credentials.edit"),
                delete: t("providers.credentials.delete"),
              }}
            />
          ) : null}
          {activeTab === "status" ? (
            <StatusTab
              rows={statusRows}
              onUpdate={(row, status) => void updateStatus(row, status)}
              labels={{
                title: t("providers.status.title"),
                meta: (row) =>
                  t("providers.status.meta", {
                    status:
                      row.status === "healthy"
                        ? t("providers.status.healthy")
                        : row.status === "dead"
                          ? t("providers.status.dead")
                          : row.status,
                    available: String(row.available),
                  }),
                healthy: t("providers.status.healthy"),
                dead: t("providers.status.dead"),
                none: t("providers.status.none"),
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
                instructions: t("providers.oauth.instructions"),
                callbackUrl: t("providers.oauth.callbackUrl"),
                callbackHint: t("providers.oauth.callbackHint"),
                persistedCredential: t("providers.oauth.persistedCredential"),
              }}
            />
          ) : null}
          {activeTab === "usage" ? (
            <UsageTab
              title={t("providers.usage.title")}
              result={usageResult}
              onRefresh={() => void loadUsage()}
              label={t("providers.usage.fetch")}
            />
          ) : null}
        </div>
      </div>
    </div>
  );
}
