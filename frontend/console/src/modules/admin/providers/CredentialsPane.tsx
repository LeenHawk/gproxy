import { useEffect, useMemo, useState } from "react";

import { useI18n } from "../../../app/i18n";
import { apiJson, apiVoid } from "../../../lib/api";
import { authHeaders } from "../../../lib/auth";
import type {
  CredentialHealthRow,
  CredentialRow,
  ProviderRow,
} from "../../../lib/types/admin";
import {
  buildCredentialJson,
  credentialValuesFromJson,
  emptyCredentialValuesForChannel,
} from "./channel-forms";
import { CredentialsTab } from "./CredentialsTab";
import type { CredentialFormState } from "./index";
import {
  parseLiveUsageRows,
  supportsCredentialUsageChannel,
  type LiveUsageRow,
} from "./usage";

/// Credentials tab container. Owns the editor form state, live-usage cache,
/// and all credential/status mutation handlers. Calls `onProviderScopedReload`
/// to refresh the shared credentialRows/statusRows owned by the parent.
export function CredentialsPane({
  selectedProvider,
  formChannel,
  credentialRows,
  statusRows,
  sessionToken,
  notify,
  onProviderScopedReload,
}: {
  selectedProvider: ProviderRow | null;
  /// The provider form's current channel. Used when selectedProvider is null
  /// (new-provider flow) so the credential editor knows which fields to show.
  formChannel: string;
  credentialRows: CredentialRow[];
  statusRows: CredentialHealthRow[];
  sessionToken: string;
  notify: (kind: "success" | "error" | "info", message: string) => void;
  onProviderScopedReload: (provider: ProviderRow) => Promise<void>;
}) {
  const { t } = useI18n();
  const headers = useMemo(() => authHeaders(sessionToken), [sessionToken]);

  const [credentialForm, setCredentialForm] = useState<CredentialFormState>({
    values: emptyCredentialValuesForChannel(formChannel),
    editingIndex: null,
    rawJson: "",
  });
  const [usageByCredential, setUsageByCredential] = useState<Record<number, string>>({});
  const [usageRowsByCredential, setUsageRowsByCredential] = useState<
    Record<number, LiveUsageRow[]>
  >({});
  const [usageLoadingByCredential, setUsageLoadingByCredential] = useState<
    Record<number, boolean>
  >({});

  // Reset editor + usage cache whenever the provider or its channel changes.
  // Matches the original ProvidersModule effect on [selectedProvider?.id,
  // providerForm.channel].
  useEffect(() => {
    setCredentialForm({
      values: emptyCredentialValuesForChannel(formChannel),
      editingIndex: null,
      rawJson: "",
    });
    setUsageByCredential({});
    setUsageRowsByCredential({});
    setUsageLoadingByCredential({});
  }, [selectedProvider?.id, formChannel]);

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
      await onProviderScopedReload(selectedProvider);
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
        await onProviderScopedReload(selectedProvider);
      }
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
        await onProviderScopedReload(selectedProvider);
      }
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

  const activeChannel = selectedProvider?.channel ?? formChannel;

  return (
    <CredentialsTab
      channel={activeChannel}
      credentials={credentialRows}
      form={credentialForm}
      onChangeForm={setCredentialForm}
      onEdit={editCredential}
      onNew={() =>
        setCredentialForm({
          values: emptyCredentialValuesForChannel(activeChannel),
          editingIndex: null,
          rawJson: "",
        })
      }
      onDelete={(row) => void deleteCredential(row)}
      onSave={() => void saveCredential()}
      statuses={statusRows}
      onUpdateStatus={(row, status) => void updateStatus(row, status)}
      supportsUsage={supportsCredentialUsageChannel(activeChannel)}
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
  );
}
