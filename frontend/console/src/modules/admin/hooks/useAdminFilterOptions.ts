import { useCallback, useEffect, useMemo, useState } from "react";

import { apiJson } from "../../../lib/api";
import type {
  CredentialRow,
  MemoryUserKeyRow,
  MemoryUserRow,
  ProviderRow,
} from "../../../lib/types/admin";

type SelectOption = { value: string; label: string };
type NotifyFn = (kind: "success" | "error" | "info", message: string) => void;
type TranslateFn = (key: string, params?: Record<string, string | number>) => string;

/// Centralised loader for the four "filter dropdown" data sources used by
/// every admin module that filters by provider / credential / user / user
/// key. Replaces ~80 lines of duplicated `useEffect + Promise.all` boilerplate
/// previously living in RequestsModule and UsageModule.
///
/// Returns:
/// - the raw rows (so callers can build other derived selectors)
/// - pre-built `SelectOption[]` lists for the four common dropdowns, each
///   prefixed with an "All" option
/// - filter helpers that narrow `userKeys` by `selectedUserId` and
///   `credentials` by `selectedProviderId`, matching the cascading-filter
///   behaviour the admin UI expects
/// - `isLoading` for spinners and `reload` to manually re-fetch
export function useAdminFilterOptions({
  headers,
  notify,
  t,
}: {
  headers: HeadersInit;
  notify: NotifyFn;
  t: TranslateFn;
}) {
  const [providers, setProviders] = useState<ProviderRow[]>([]);
  const [credentials, setCredentials] = useState<CredentialRow[]>([]);
  const [users, setUsers] = useState<MemoryUserRow[]>([]);
  const [userKeys, setUserKeys] = useState<MemoryUserKeyRow[]>([]);
  const [isLoading, setIsLoading] = useState(false);

  const reload = useCallback(async () => {
    setIsLoading(true);
    try {
      const [providerRows, credentialRows, userRows, userKeyRows] = await Promise.all([
        apiJson<ProviderRow[]>("/admin/providers/query", {
          method: "POST",
          headers,
          body: JSON.stringify({}),
        }),
        apiJson<CredentialRow[]>("/admin/credentials/query", {
          method: "POST",
          headers,
          body: JSON.stringify({}),
        }),
        apiJson<MemoryUserRow[]>("/admin/users/query", {
          method: "POST",
          headers,
          body: JSON.stringify({}),
        }),
        apiJson<MemoryUserKeyRow[]>("/admin/user-keys/query", {
          method: "POST",
          headers,
          body: JSON.stringify({}),
        }),
      ]);
      setProviders(providerRows);
      setCredentials(credentialRows);
      setUsers(userRows);
      setUserKeys(userKeyRows);
    } catch (error) {
      notify("error", error instanceof Error ? error.message : String(error));
    } finally {
      setIsLoading(false);
    }
  }, [headers, notify]);

  useEffect(() => {
    void reload();
  }, [reload]);

  const providerOptions = useMemo<SelectOption[]>(
    () => [
      { value: "", label: `${t("common.all")} ${t("common.provider")}` },
      ...providers.map((row) => ({ value: String(row.id), label: `${row.name} (#${row.id})` })),
    ],
    [providers, t],
  );

  const userOptions = useMemo<SelectOption[]>(
    () => [
      { value: "", label: `${t("common.all")} ${t("common.user")}` },
      ...users.map((row) => ({ value: String(row.id), label: `${row.name} (#${row.id})` })),
    ],
    [users, t],
  );

  const credentialOptionsBuilder = useCallback(
    (selectedProviderId: string): SelectOption[] => {
      const filtered = !selectedProviderId
        ? credentials
        : credentials.filter((row) => {
            const provider = providers.find((p) => p.name === row.provider);
            return provider ? String(provider.id) === selectedProviderId : false;
          });
      return [
        { value: "", label: `${t("common.all")} ${t("providers.tab.credentials")}` },
        ...filtered.map((row) => ({
          value: String(row.id),
          label: `${row.provider} #${row.index}`,
        })),
      ];
    },
    [credentials, providers, t],
  );

  const userKeyOptionsBuilder = useCallback(
    (selectedUserId: string): SelectOption[] => {
      const filtered = !selectedUserId
        ? userKeys
        : userKeys.filter((row) => String(row.user_id) === selectedUserId);
      return [
        { value: "", label: `${t("common.all")} ${t("app.nav.myKeys")}` },
        ...filtered.map((row) => ({
          value: String(row.id),
          label: `#${row.id} · user #${row.user_id}`,
        })),
      ];
    },
    [userKeys, t],
  );

  return {
    providers,
    credentials,
    users,
    userKeys,
    providerOptions,
    userOptions,
    credentialOptionsBuilder,
    userKeyOptionsBuilder,
    isLoading,
    reload,
  };
}
