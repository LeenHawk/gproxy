import { useEffect, useMemo, useState } from "react";

import { useI18n } from "../../../app/i18n";
import { useBatchSelection } from "../../../components/useBatchSelection";
import { apiJson, apiVoid } from "../../../lib/api";
import { authHeaders } from "../../../lib/auth";
import { parseRequiredI64 } from "../../../lib/form";
import type {
  MemoryModelRow,
  ModelWrite,
  ProviderRow,
  ProviderWrite,
} from "../../../lib/types/admin";
import { buildChannelSettingsJson } from "./channel-forms";
import { buildDispatchDocument } from "./dispatch";
import { ModelsTab, type ModelFormState } from "./ModelsTab";
import { filterModelsForProvider, nextResourceId } from "./resources";
import type { ProviderFormState } from "./index";
import type { SuffixActionSetBody } from "./suffix-presets";

/// Models tab container. Owns the model-edit form, selection, and the
/// models-batch selection. Reads `allModelRows` from the parent's
/// `useProviderModels` hook so the rewrite-tab autocomplete can use the
/// same list without a second fetch. `addSuffixVariant` is the one
/// handler that reaches back into providerForm — it writes merged rewrite
/// rules via `updateProviderForm` and persists them via the upsert API.
export function ModelsPane({
  selectedProvider,
  providerForm,
  updateProviderForm,
  allModelRows,
  reloadModels,
  setAllModelRows,
  sessionToken,
  notify,
}: {
  selectedProvider: ProviderRow | null;
  providerForm: ProviderFormState;
  updateProviderForm: (patch: Partial<ProviderFormState>) => void;
  allModelRows: MemoryModelRow[];
  reloadModels: () => Promise<void>;
  setAllModelRows: (rows: MemoryModelRow[]) => void;
  sessionToken: string;
  notify: (kind: "success" | "error" | "info", message: string) => void;
}) {
  const { t } = useI18n();
  const headers = useMemo(() => authHeaders(sessionToken), [sessionToken]);

  const [selectedModelId, setSelectedModelId] = useState<number | null>(null);
  const [modelForm, setModelForm] = useState<ModelFormState>({
    id: "",
    model_id: "",
    display_name: "",
    enabled: true,
    pricing_json: "",
    alias_of: "",
  });

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

  // Clear model selection whenever the provider or its channel changes.
  // Matches the original ProvidersModule effect on [selectedProvider?.id,
  // providerForm.channel].
  useEffect(() => {
    setSelectedModelId(null);
  }, [selectedProvider?.id, providerForm.channel]);

  // When the model list changes or selection was cleared (e.g. after delete),
  // reset the form to a blank "new model" state. Preserves the original
  // effect's dependency list verbatim.
  // eslint-disable-next-line react-hooks/exhaustive-deps
  useEffect(() => {
    if (!selectedProvider) {
      return;
    }
    if (selectedModelId === null) {
      beginCreateModel();
    }
  }, [allModelRows, selectedModelId, selectedProvider?.id]);

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
    actions: SuffixActionSetBody[],
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
        label: providerForm.label.trim() || null,
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

  return (
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
          sectionTitle: t("models.pricingJson"),
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
  );
}
