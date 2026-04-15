import { useCallback, useEffect, useMemo, useState } from "react";

import { apiJson } from "../../../../lib/api";
import { authHeaders } from "../../../../lib/auth";
import type { MemoryModelRow } from "../../../../lib/types/admin";

/// Owns the provider-scoped "all models" list. Fetches on provider change so
/// sibling panes (ModelsPane, the rewrite rule autocomplete, etc.) can read
/// the same data without each tab re-fetching or racing. Kept as a shared
/// hook rather than a Context to stay consistent with the existing
/// `useProviderData` pattern.
export function useProviderModels({
  sessionToken,
  selectedProviderId,
  notify,
}: {
  sessionToken: string;
  selectedProviderId: number | null | undefined;
  notify: (kind: "success" | "error" | "info", message: string) => void;
}) {
  const headers = useMemo(() => authHeaders(sessionToken), [sessionToken]);
  const [allModelRows, setAllModelRows] = useState<MemoryModelRow[]>([]);

  const reloadModels = useCallback(async () => {
    const models = await apiJson<MemoryModelRow[]>("/admin/models/query", {
      method: "POST",
      headers,
      body: JSON.stringify({}),
    });
    setAllModelRows(models);
  }, [headers]);

  useEffect(() => {
    if (!selectedProviderId) {
      setAllModelRows([]);
      return;
    }
    let active = true;
    void apiJson<MemoryModelRow[]>("/admin/models/query", {
      method: "POST",
      headers,
      body: JSON.stringify({}),
    })
      .then((models) => {
        if (!active) return;
        setAllModelRows(models);
      })
      .catch((error) => {
        if (!active) return;
        notify("error", error instanceof Error ? error.message : String(error));
      });
    return () => {
      active = false;
    };
  }, [headers, notify, selectedProviderId]);

  return { allModelRows, setAllModelRows, reloadModels };
}
