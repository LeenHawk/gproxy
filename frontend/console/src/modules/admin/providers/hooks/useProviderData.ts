import { useCallback, useEffect, useMemo, useState } from "react";

import { apiJson } from "../../../../lib/api";
import { authHeaders } from "../../../../lib/auth";
import type {
  CredentialHealthRow,
  CredentialRow,
  ProviderRow,
} from "../../../../lib/types/admin";
import {
  defaultSettingsForChannel,
  settingsValuesFromJson,
} from "../channel-forms";
import { createRoutingRuleDraft, routingDraftsFromDocument } from "../routing";
import type { ProviderFormState } from "../index";

function formFromProvider(row: ProviderRow): ProviderFormState {
  return {
    id: String(row.id),
    name: row.name,
    label: row.label ?? "",
    channel: row.channel,
    settings: settingsValuesFromJson(row.channel, row.settings_json),
    routingRules: routingDraftsFromDocument(row.routing_json),
  };
}

function emptyForm(nextId: number): ProviderFormState {
  return {
    id: String(nextId),
    name: "",
    label: "",
    channel: "openai",
    settings: defaultSettingsForChannel("openai"),
    routingRules: [createRoutingRuleDraft()],
  };
}

export function useProviderData(sessionToken: string) {
  const [providerRows, setProviderRows] = useState<ProviderRow[]>([]);
  const [selectedProviderId, setSelectedProviderId] = useState<number | null>(null);
  const [providerForm, setProviderForm] = useState<ProviderFormState>(emptyForm(1));
  const [credentialRows, setCredentialRows] = useState<CredentialRow[]>([]);
  const [statusRows, setStatusRows] = useState<CredentialHealthRow[]>([]);
  const headers = useMemo(() => authHeaders(sessionToken), [sessionToken]);

  const selectedProvider =
    providerRows.find((row) => row.id === selectedProviderId) ?? null;

  const loadProviderScopedData = useCallback(
    async (provider: ProviderRow | null) => {
      if (!provider) {
        setCredentialRows([]);
        setStatusRows([]);
        return;
      }
      const [credentials, statuses] = await Promise.all([
        apiJson<CredentialRow[]>("/admin/credentials/query", {
          method: "POST",
          headers,
          body: JSON.stringify({ provider_name: { Eq: provider.name } }),
        }),
        apiJson<CredentialHealthRow[]>("/admin/credential-statuses/query", {
          method: "POST",
          headers,
          body: JSON.stringify({ provider_name: { Eq: provider.name } }),
        }),
      ]);
      setCredentialRows(credentials);
      setStatusRows(statuses);
    },
    [headers],
  );

  const loadProviders = useCallback(async () => {
    const rows = await apiJson<ProviderRow[]>("/admin/providers/query", {
      method: "POST",
      headers,
      body: JSON.stringify({}),
    });
    const sorted = [...rows].sort((left, right) => left.id - right.id);
    setProviderRows(sorted);
    return sorted;
  }, [headers]);

  useEffect(() => {
    let active = true;
    void loadProviders().then(async (rows) => {
      if (!active) {
        return;
      }
      if (rows.length > 0) {
        const first = rows[0];
        setSelectedProviderId(first.id);
        setProviderForm(formFromProvider(first));
        await loadProviderScopedData(first);
      } else {
        setProviderForm(emptyForm(1));
      }
    });
    return () => {
      active = false;
    };
  }, [loadProviderScopedData, loadProviders]);

  const selectProvider = useCallback(
    async (row: ProviderRow) => {
      setSelectedProviderId(row.id);
      setProviderForm(formFromProvider(row));
      await loadProviderScopedData(row);
    },
    [loadProviderScopedData],
  );

  const beginCreateProvider = useCallback(() => {
    const nextId = providerRows.reduce((max, row) => Math.max(max, row.id), 0) + 1;
    setSelectedProviderId(null);
    setCredentialRows([]);
    setStatusRows([]);
    setProviderForm(emptyForm(nextId));
  }, [providerRows]);

  const reloadAndReselect = useCallback(
    async (providerName?: string) => {
      const latest = await loadProviders();
      const next =
        (providerName
          ? latest.find((row) => row.name === providerName)
          : null) ??
        latest.find((row) => row.id === selectedProviderId) ??
        latest[0] ??
        null;
      if (next) {
        await selectProvider(next);
      } else {
        beginCreateProvider();
      }
    },
    [beginCreateProvider, loadProviders, selectProvider, selectedProviderId],
  );

  return {
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
  };
}
