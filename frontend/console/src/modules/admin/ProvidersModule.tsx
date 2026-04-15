import { useEffect, useMemo, useState } from "react";

import { useI18n } from "../../app/i18n";
import { Button } from "../../components/ui";
import { apiJson } from "../../lib/api";
import { authHeaders } from "../../lib/auth";
import { parseRequiredI64 } from "../../lib/form";
import type {
  DispatchTableDocument,
  ProviderDispatchTemplateParams,
  ProviderWrite,
} from "../../lib/types/admin";
import { buildChannelSettingsJson, defaultSettingsForChannel } from "./providers/channel-forms";
import {
  buildDispatchDocument,
  createDispatchRuleDraft,
  dispatchDraftsFromDocument,
} from "./providers/dispatch";
import { ConfigTab } from "./providers/ConfigTab";
import { CredentialsPane } from "./providers/CredentialsPane";
import { ModelsPane } from "./providers/ModelsPane";
import { OAuthPane } from "./providers/OAuthPane";
import { ProviderList } from "./providers/ProviderList";
import { RewriteRulesTab } from "./providers/RewriteRulesTab";
import { filterModelsForProvider } from "./providers/resources";
import { useProviderData } from "./providers/hooks/useProviderData";
import { useProviderModels } from "./providers/hooks/useProviderModels";
import type { ProviderFormState, ProviderWorkspaceTab } from "./providers";

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
  const { allModelRows, setAllModelRows, reloadModels } = useProviderModels({
    sessionToken,
    selectedProviderId: selectedProvider?.id ?? null,
    notify,
  });
  const [activeTab, setActiveTab] = useState<ProviderWorkspaceTab>("config");

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

  const updateProviderForm = (patch: Partial<ProviderFormState>) => {
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

  // When creating a new provider, re-load the default dispatch template on
  // channel change so the user sees a sensible starting point. Selected
  // (existing) providers already carry their dispatch rules from storage.
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
  }, [headers, notify, providerForm.channel, providerForm.id, selectedProvider, setProviderForm]);

  const saveProvider = async () => {
    try {
      const payload: ProviderWrite = {
        id: parseRequiredI64(providerForm.id, "id"),
        name: providerForm.name.trim(),
        channel: providerForm.channel.trim(),
        label: providerForm.label.trim() || null,
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

  const onOAuthFinished = async () => {
    if (selectedProvider) {
      await loadProviderScopedData(selectedProvider);
    }
    setActiveTab("credentials");
  };

  const providerModelRows = useMemo(
    () => filterModelsForProvider(allModelRows, selectedProvider?.id ?? null),
    [allModelRows, selectedProvider?.id],
  );

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
                label: t("providers.form.label"),
                labelPlaceholder: t("providers.form.labelPlaceholder"),
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
            <CredentialsPane
              selectedProvider={selectedProvider}
              formChannel={providerForm.channel}
              credentialRows={credentialRows}
              statusRows={statusRows}
              sessionToken={sessionToken}
              notify={notify}
              onProviderScopedReload={loadProviderScopedData}
            />
          ) : null}
          {activeTab === "models" ? (
            <ModelsPane
              selectedProvider={selectedProvider}
              providerForm={providerForm}
              updateProviderForm={updateProviderForm}
              allModelRows={allModelRows}
              reloadModels={reloadModels}
              setAllModelRows={setAllModelRows}
              sessionToken={sessionToken}
              notify={notify}
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
            <OAuthPane
              selectedProvider={selectedProvider}
              sessionToken={sessionToken}
              notify={notify}
              onFinished={() => void onOAuthFinished()}
            />
          ) : null}
        </div>
      </div>
    </div>
  );
}
