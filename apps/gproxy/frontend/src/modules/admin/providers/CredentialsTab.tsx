import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type ChangeEvent,
  type Dispatch,
  type SetStateAction
} from "react";

import type {
  CredentialQueryRow,
  CredentialStatusQueryRow,
  ProviderQueryRow
} from "../../../lib/types";
import { Button, Input, Label, Select, TextArea } from "../../../components/ui";
import { CredentialBulkSection } from "./credentials-tab/CredentialBulkSection";
import { CredentialCardsSection } from "./credentials-tab/CredentialCardsSection";
import { CredentialSingleSection } from "./credentials-tab/CredentialSingleSection";
import type { CooldownItem, CredentialHealthKind, TranslateFn } from "./credentials-tab/shared";
import {
  availableBulkModes,
  buildBulkExportText,
  defaultBulkMode,
  getChannelConfig,
  parseBulkCredentialText,
  type BulkCredentialImportEntry,
  type CredentialBulkMode,
  type LiveUsageRow,
  type UsageDisplayKind,
  type UsageDisplayRow,
  ChannelCredentialSchema,
  CredentialFieldSchema,
  CredentialFieldValue,
  CredentialFormState,
  StatusFormState
} from "./index";

type OAuthReadableResult = {
  authUrl?: string;
  verificationUri?: string;
  userCode?: string;
  interval?: string;
  instructions?: string;
};

type CredentialSearchMode = "id" | "name";

export type CredentialsTabMode = "bulk" | "oauth" | "list";

export type CredentialsTabViewModel = {
  selectedProvider: ProviderQueryRow | null;
  credentialSchema: ChannelCredentialSchema;
  supportsUpstreamUsage: boolean;
  supportsOAuth: boolean;
  deadCredentialCount: number;
  testingAllCredentials: boolean;
  credentialTotalCount: number;
  credentialFilteredCount: number;
  credentialPageSize: number;
  credentialPage: number;
  credentialSearchMode: CredentialSearchMode;
  credentialSearchText: string;
  credentialListLoading: boolean;
  credentialRows: CredentialQueryRow[];
  statusesByCredential: Map<number, CredentialStatusQueryRow[]>;
  usageByCredential: Record<number, string>;
  liveUsageRowsByCredential: Record<number, LiveUsageRow[]>;
  usageDisplayKindByCredential: Record<number, UsageDisplayKind>;
  usageDisplayRowsByCredential: Record<number, UsageDisplayRow[]>;
  usageLoadingByCredential: Record<number, boolean>;
  usageErrorByCredential: Record<number, string>;
  oauthStartQueryByCredential: Record<number, string>;
  oauthCallbackQueryByCredential: Record<number, string>;
  oauthActiveModeByCredential: Record<number, string>;
  oauthResultByCredential: Record<number, string>;
  statusEditorCredentialId: number | null;
  statusForm: StatusFormState;
  credentialForm: CredentialFormState;
};

export type CredentialsTabActions = {
  setOauthStartQueryByCredential: Dispatch<SetStateAction<Record<number, string>>>;
  setOauthCallbackQueryByCredential: Dispatch<SetStateAction<Record<number, string>>>;
  setStatusEditorCredentialId: Dispatch<SetStateAction<number | null>>;
  setStatusForm: Dispatch<SetStateAction<StatusFormState>>;
  setCredentialForm: Dispatch<SetStateAction<CredentialFormState>>;
  onChangeCredentialSearchMode: (value: CredentialSearchMode) => void;
  onChangeCredentialSearchText: (value: string) => void;
  onChangeCredentialPageSize: (value: number) => void;
  onChangeCredentialPage: (value: number) => void;
  onEditCredential: (row: CredentialQueryRow) => void;
  onCopyCredential: (row: CredentialQueryRow) => void;
  onRemoveCredential: (id: number) => void;
  onRequestRemoveDeadCredentials: () => void;
  onTestAllCredentials: () => void;
  onToggleCredentialEnabled: (row: CredentialQueryRow) => void;
  onSetCredentialHealth: (payload: {
    credentialId: number;
    statusId?: number;
    healthKind: CredentialHealthKind;
    healthJson: Record<string, unknown> | null;
    lastError?: string | null;
  }) => void;
  onQueryUpstreamUsage: (credentialId: number) => void;
  onUpsertStatus: () => void;
  onRunCredentialOAuthStart: (
    credentialId?: number,
    mode?: string,
    queryDefaults?: Record<string, string | null | undefined>
  ) => void;
  onRunCredentialOAuthCallback: (
    credentialId?: number,
    mode?: string,
    queryDefaults?: Record<string, string | null | undefined>
  ) => void;
  onUpsertCredential: () => void;
  onUpsertCredentialsBatch: (entries: BulkCredentialImportEntry[]) => void | Promise<void>;
};

function parseOAuthReadableResult(raw: string): OAuthReadableResult | null {
  const text = raw.trim();
  if (!text) {
    return null;
  }
  let parsed: unknown;
  try {
    parsed = JSON.parse(text);
  } catch {
    return null;
  }

  if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
    return null;
  }

  const object = parsed as Record<string, unknown>;
  const pick = (key: string): string | undefined => {
    const value = object[key];
    if (typeof value !== "string") {
      return undefined;
    }
    const trimmed = value.trim();
    return trimmed ? trimmed : undefined;
  };
  const pickNumberAsString = (key: string): string | undefined => {
    const value = object[key];
    if (typeof value === "number" && Number.isFinite(value)) {
      return String(value);
    }
    if (typeof value === "string") {
      const trimmed = value.trim();
      return trimmed ? trimmed : undefined;
    }
    return undefined;
  };

  return {
    authUrl: pick("auth_url"),
    verificationUri: pick("verification_uri"),
    userCode: pick("user_code"),
    interval: pickNumberAsString("interval"),
    instructions: pick("instructions")
  };
}

export function CredentialsTab({
  mode,
  viewModel,
  actions,
  t
}: {
  mode: CredentialsTabMode;
  viewModel: CredentialsTabViewModel;
  actions: CredentialsTabActions;
  t: TranslateFn;
}) {
  const {
    selectedProvider,
    credentialSchema,
    supportsUpstreamUsage,
    supportsOAuth,
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
  } = viewModel;
  const {
    setOauthStartQueryByCredential,
    setOauthCallbackQueryByCredential,
    setStatusEditorCredentialId,
    setStatusForm,
    setCredentialForm,
    onChangeCredentialSearchMode,
    onChangeCredentialSearchText,
    onChangeCredentialPageSize,
    onChangeCredentialPage,
    onEditCredential,
    onCopyCredential,
    onRemoveCredential,
    onRequestRemoveDeadCredentials,
    onTestAllCredentials,
    onToggleCredentialEnabled,
    onSetCredentialHealth,
    onQueryUpstreamUsage,
    onUpsertStatus,
    onRunCredentialOAuthStart,
    onRunCredentialOAuthCallback,
    onUpsertCredential,
    onUpsertCredentialsBatch
  } = actions;

  if (!selectedProvider) {
    return <p className="text-sm text-muted">{t("providers.needProvider")}</p>;
  }

  const resolveFieldLabel = (field: CredentialFieldSchema): string => {
    const key = `field.${field.key}`;
    const translated = t(key);
    return translated === key ? field.label : translated;
  };

  const setSecretField = (key: string, value: CredentialFieldValue) => {
    setCredentialForm((prev) => ({
      ...prev,
      secretValues: {
        ...prev.secretValues,
        [key]: value
      }
    }));
  };

  const channel = selectedProvider.channel.trim().toLowerCase();
  const GLOBAL_OAUTH_SLOT = 0;
  const oauthUi = getChannelConfig(channel)?.oauthUi;
  const oauthStartButtons = oauthUi?.startButtons ?? [{ labelKey: "providers.oauth.start" }];
  const oauthCallbackButtons = oauthUi?.callbackButtons ?? [{ labelKey: "providers.oauth.callback" }];
  const bulkModes = useMemo(
    () => availableBulkModes(channel, credentialSchema, supportsOAuth),
    [channel, credentialSchema, supportsOAuth]
  );
  const [bulkMode, setBulkMode] = useState<CredentialBulkMode>(
    defaultBulkMode(channel, credentialSchema, supportsOAuth)
  );
  const [expandedCooldownCredentialId, setExpandedCooldownCredentialId] = useState<number | null>(
    null
  );
  const [selectedCooldownKeysByCredential, setSelectedCooldownKeysByCredential] = useState<
    Record<number, string[]>
  >({});
  const [bulkInputText, setBulkInputText] = useState("");
  const [bulkExportText, setBulkExportText] = useState("");
  const [bulkError, setBulkError] = useState("");
  const [listEditorCredentialId, setListEditorCredentialId] = useState<number | null>(null);
  const bulkFileInputRef = useRef<HTMLInputElement | null>(null);
  const oauthReadableResult = useMemo(
    () => parseOAuthReadableResult(oauthResultByCredential[GLOBAL_OAUTH_SLOT] ?? ""),
    [oauthResultByCredential]
  );
  const oauthActiveMode = oauthActiveModeByCredential[GLOBAL_OAUTH_SLOT];
  const oauthStartQuery = oauthStartQueryByCredential[GLOBAL_OAUTH_SLOT] ?? "";
  const oauthCallbackQuery = oauthCallbackQueryByCredential[GLOBAL_OAUTH_SLOT] ?? "";
  const oauthRawResult = oauthResultByCredential[GLOBAL_OAUTH_SLOT] ?? "";
  const oauthOpenUrl = oauthReadableResult?.authUrl ?? oauthReadableResult?.verificationUri;
  const oauthCallbackModeCount = new Set(
    oauthCallbackButtons.flatMap((button) => (button.mode ? [button.mode] : []))
  ).size;
  const oauthRequiresModeSelection = oauthCallbackModeCount > 1;
  const visibleOauthCallbackButtons =
    oauthRequiresModeSelection && oauthActiveMode
      ? oauthCallbackButtons.filter((button) => button.mode === oauthActiveMode)
      : oauthRequiresModeSelection
        ? []
        : oauthCallbackButtons;
  const oauthVisibleCallbackUsesCustomFields = visibleOauthCallbackButtons.some(
    (button) => Array.isArray(button.fields) && button.fields.length > 0
  );

  useEffect(() => {
    setBulkMode(defaultBulkMode(channel, credentialSchema, supportsOAuth));
    setBulkInputText("");
    setBulkExportText("");
    setBulkError("");
    setExpandedCooldownCredentialId(null);
    setSelectedCooldownKeysByCredential({});
    setListEditorCredentialId(null);
  }, [channel, credentialSchema, supportsOAuth]);

  useEffect(() => {
    if (listEditorCredentialId === null) {
      return;
    }
    if (!credentialRows.some((row) => row.id === listEditorCredentialId)) {
      setListEditorCredentialId(null);
    }
  }, [credentialRows, listEditorCredentialId]);

  const listTotalPages = Math.max(1, Math.ceil(credentialFilteredCount / credentialPageSize));

  const normalizeHealthKind = (value: string | undefined): CredentialHealthKind => {
    switch (value?.trim().toLowerCase()) {
      case "dead":
        return "dead";
      case "partial":
        return "partial";
      default:
        return "healthy";
    }
  };

  const parseCooldowns = (status: CredentialStatusQueryRow | undefined): CooldownItem[] => {
    if (!status || normalizeHealthKind(status.health_kind) !== "partial") {
      return [];
    }
    const value = status.health_json;
    if (!value || typeof value !== "object") {
      return [];
    }
    const source = Array.isArray((value as { models?: unknown }).models)
      ? ((value as { models: unknown[] }).models ?? [])
      : Array.isArray(value)
        ? value
        : [];
    const result: CooldownItem[] = [];
    for (const item of source) {
      if (!item || typeof item !== "object" || Array.isArray(item)) {
        continue;
      }
      const model =
        typeof (item as { model?: unknown }).model === "string"
          ? (item as { model: string }).model.trim()
          : "";
      const untilRaw = (item as { until_unix_ms?: unknown }).until_unix_ms;
      const untilUnixMs =
        typeof untilRaw === "number" && Number.isFinite(untilRaw) ? Math.floor(untilRaw) : NaN;
      if (model && Number.isFinite(untilUnixMs)) {
        result.push({ model, untilUnixMs });
      }
    }
    return result;
  };

  const healthLabel = (kind: CredentialHealthKind): string => {
    if (kind === "dead") {
      return t("providers.health.dead");
    }
    if (kind === "partial") {
      return t("providers.health.partial");
    }
    return t("providers.health.healthy");
  };

  const cooldownKey = (item: CooldownItem): string => `${item.model}::${item.untilUnixMs}`;

  const bulkPlaceholder = useMemo(() => {
    if (bulkMode === "keys") {
      return `${t("providers.bulk.placeholder.keyLineA")}\n${t("providers.bulk.placeholder.keyLineB")}`;
    }
    if (bulkMode === "claudecode_cookie") {
      return `${t("providers.bulk.placeholder.cookieLineA")}\n${t("providers.bulk.placeholder.cookieLineB")}`;
    }
    const sample: Record<string, unknown> = {
      name: "credential-1",
      enabled: true
    };
    for (const field of credentialSchema.fields) {
      if (field.type === "integer") {
        sample[field.key] = 0;
      } else if (field.type === "boolean") {
        sample[field.key] = true;
      } else if (field.type === "optional_boolean") {
        sample[field.key] = true;
      } else {
        sample[field.key] = `${field.key}-value`;
      }
    }
    return JSON.stringify(sample);
  }, [bulkMode, credentialSchema.fields, t]);

  const formatWindowLabel = (window: UsageDisplayRow["window"]): string => {
    if (window === "primary") {
      return t("providers.usage.window_primary");
    }
    if (window === "secondary") {
      return t("providers.usage.window_secondary");
    }
    if (window === "code_review") {
      return t("providers.usage.window_code_review");
    }
    if (window === "5h") {
      return t("providers.usage.window_5h");
    }
    if (window === "1d") {
      return t("providers.usage.window_1d");
    }
    if (window === "1w") {
      return t("providers.usage.window_1w");
    }
    return t("providers.usage.window_sum");
  };

  const resolveUsageGroupLabel = (label: string): string => {
    if (label === "all") {
      return t("providers.usage.group_all");
    }
    if (label === "haiku") {
      return t("providers.usage.group_haiku");
    }
    if (label === "sonnet") {
      return t("providers.usage.group_sonnet");
    }
    if (label === "opus") {
      return t("providers.usage.group_opus");
    }
    return label;
  };

  const resolveLiveLimitLabel = (label: string): string => {
    const key = `providers.usage.live_key.${label}`;
    const translated = t(key);
    return translated === key ? label : translated;
  };

  const runBulkImport = () => {
    try {
      const entries = parseBulkCredentialText({
        channel,
        schema: credentialSchema,
        mode: bulkMode,
        rawText: bulkInputText
      });
      setBulkError("");
      onUpsertCredentialsBatch(entries);
    } catch (error) {
      setBulkError(error instanceof Error ? error.message : String(error));
    }
  };

  const runBulkExport = () => {
    const text = buildBulkExportText({
      channel,
      schema: credentialSchema,
      mode: bulkMode,
      credentialRows
    });
    setBulkExportText(text);
  };

  const openBulkImportFilePicker = () => {
    bulkFileInputRef.current?.click();
  };

  const onBulkImportFileChange = async (event: ChangeEvent<HTMLInputElement>) => {
    const files = Array.from(event.target.files ?? []);
    event.target.value = "";
    if (files.length === 0) {
      return;
    }

    try {
      const fileEntryGroups = await Promise.all(
        files.map(async (file) => {
          try {
            const text = await file.text();
            return parseBulkCredentialText({
              channel,
              schema: credentialSchema,
              mode: bulkMode,
              rawText: text
            });
          } catch (error) {
            const message = error instanceof Error ? error.message : String(error);
            throw new Error(`${file.name}: ${message}`);
          }
        })
      );
      setBulkInputText("");
      setBulkError("");
      await onUpsertCredentialsBatch(fileEntryGroups.flat());
    } catch (error) {
      setBulkError(error instanceof Error ? error.message : String(error));
    }
  };

  const runBulkExportFile = () => {
    const text = buildBulkExportText({
      channel,
      schema: credentialSchema,
      mode: bulkMode,
      credentialRows
    });
    setBulkExportText(text);

    if (!text.trim()) {
      setBulkError(t("providers.bulk.emptyExport"));
      return;
    }

    const extension = bulkMode === "json" ? "jsonl" : "txt";
    const stamp = new Date().toISOString().replace(/[:.]/g, "-");
    const filename = `gproxy-credentials-${channel}-${stamp}.${extension}`;
    const blob = new Blob([text], { type: "text/plain;charset=utf-8" });
    const url = URL.createObjectURL(blob);
    const anchor = document.createElement("a");
    anchor.href = url;
    anchor.download = filename;
    document.body.append(anchor);
    anchor.click();
    anchor.remove();
    URL.revokeObjectURL(url);
  };

  function readQueryParam(rawQuery: string, key: string): string {
    const input = rawQuery.trim();
    const params = new URLSearchParams(input.startsWith("?") ? input.slice(1) : input);
    return params.get(key) ?? "";
  }

  const oauthQueryParamName = (field: string): string => {
    if (field === "callback_code") {
      return "code";
    }
    return field;
  };

  const resolveCallbackFields = (fields?: readonly string[]): readonly string[] => {
    if (Array.isArray(fields) && fields.length > 0) {
      return fields;
    }
    return oauthUi?.callbackFields ?? [];
  };

  const updateQueryParam = (rawQuery: string, key: string, value: string): string => {
    const input = rawQuery.trim();
    const params = new URLSearchParams(input.startsWith("?") ? input.slice(1) : input);
    const trimmed = value.trim();
    if (trimmed) {
      params.set(key, trimmed);
    } else {
      params.delete(key);
    }
    const query = params.toString();
    return query ? `?${query}` : "";
  };

  const setOAuthStartParam = (key: string, value: string) => {
    setOauthStartQueryByCredential((prev) => ({
      ...prev,
      [GLOBAL_OAUTH_SLOT]: updateQueryParam(prev[GLOBAL_OAUTH_SLOT] ?? "", key, value)
    }));
  };

  const setOAuthCallbackParam = (key: string, value: string) => {
    setOauthCallbackQueryByCredential((prev) => ({
      ...prev,
      [GLOBAL_OAUTH_SLOT]: updateQueryParam(prev[GLOBAL_OAUTH_SLOT] ?? "", key, value)
    }));
  };

  const renderOAuthField = (
    kind: "start" | "callback",
    field: string,
    rawQuery: string
  ) => {
    const fieldKey = oauthQueryParamName(field);
    const labelKey = `field.${field}`;
    const translated = t(labelKey);
    const placeholder =
      kind === "start" ? oauthUi?.startDefaults?.[field] : oauthUi?.callbackDefaults?.[field];
    return (
      <div key={`${kind}-${field}`}>
        <Label>{translated === labelKey ? field : translated}</Label>
        <Input
          value={readQueryParam(rawQuery, fieldKey)}
          onChange={(value) =>
            kind === "start"
              ? setOAuthStartParam(fieldKey, value)
              : setOAuthCallbackParam(fieldKey, value)
          }
          placeholder={placeholder}
        />
      </div>
    );
  };

  const renderCredentialField = (field: CredentialFieldSchema) => {
    const value = credentialForm.secretValues[field.key];
    if (field.type === "boolean") {
      return (
        <div key={field.key} className="flex items-end gap-2 pb-2">
          <input
            id={`credential-${field.key}`}
            type="checkbox"
            checked={value === true}
            onChange={(event) => setSecretField(field.key, event.target.checked)}
          />
          <label htmlFor={`credential-${field.key}`} className="text-sm text-muted">
            {resolveFieldLabel(field)}
          </label>
        </div>
      );
    }
    if (field.type === "optional_boolean") {
      const selected = value === true ? "true" : value === false ? "false" : "";
      return (
        <div key={field.key}>
          <Label>{resolveFieldLabel(field)}</Label>
          <Select
            value={selected}
            onChange={(next) =>
              setSecretField(field.key, next === "true" ? true : next === "false" ? false : null)
            }
            options={[
              { value: "", label: t("common.unset") },
              { value: "true", label: t("common.true") },
              { value: "false", label: t("common.false") }
            ]}
          />
        </div>
      );
    }
    return (
      <div key={field.key}>
        <Label>{resolveFieldLabel(field)}</Label>
        <Input
          value={typeof value === "string" ? value : ""}
          onChange={(next) => setSecretField(field.key, next)}
          placeholder={field.placeholder}
        />
      </div>
    );
  };

  const oauthSection = supportsOAuth ? (
    <div className="provider-card space-y-2">
      <div className="text-xs font-semibold uppercase tracking-[0.08em] text-muted">
        {t("providers.section.oauth")}
      </div>
      {oauthUi?.startFields.map((field) => renderOAuthField("start", field, oauthStartQuery))}
      <div className="flex flex-wrap gap-2">
        {oauthStartButtons.map((button) => (
          <Button
            key={button.labelKey}
            variant={button.mode ? "neutral" : "primary"}
            onClick={() => onRunCredentialOAuthStart(undefined, button.mode, button.queryDefaults)}
          >
            {t(button.labelKey)}
          </Button>
        ))}
        {oauthOpenUrl ? (
          <a
            className="btn btn-primary inline-flex"
            href={oauthOpenUrl}
            target="_blank"
            rel="noopener noreferrer"
          >
            {t("providers.oauth.openAuthUrl")}
          </a>
        ) : null}
      </div>
      {!oauthRequiresModeSelection || visibleOauthCallbackButtons.length > 0 ? (
        <>
          {!oauthVisibleCallbackUsesCustomFields
            ? oauthUi?.callbackFields.map((field) =>
                renderOAuthField("callback", field, oauthCallbackQuery)
              )
            : null}
          {!oauthVisibleCallbackUsesCustomFields ? (
            <div className="flex flex-wrap gap-2">
              {visibleOauthCallbackButtons.map((button) => (
                <Button
                  key={button.labelKey}
                  variant={button.mode ? "neutral" : "primary"}
                  onClick={() =>
                    onRunCredentialOAuthCallback(undefined, button.mode, button.queryDefaults)
                  }
                >
                  {t(button.labelKey)}
                </Button>
              ))}
            </div>
          ) : null}
        </>
      ) : (
        <div className="text-sm text-muted">{t("providers.oauth.selectStartMode")}</div>
      )}
      {oauthVisibleCallbackUsesCustomFields ? (
        <div className="space-y-2">
          {visibleOauthCallbackButtons.map((button) => {
            const fields = resolveCallbackFields(button.fields);
            return (
              <div
                key={`callback-${button.labelKey}`}
                className="space-y-2 rounded-lg border border-border p-3"
              >
                <div className="text-xs font-semibold uppercase tracking-[0.08em] text-muted">
                  {t(button.labelKey)}
                </div>
                {fields.map((field) => renderOAuthField("callback", field, oauthCallbackQuery))}
                <div className="flex flex-wrap gap-2">
                  <Button
                    variant={button.mode ? "neutral" : "primary"}
                    onClick={() => onRunCredentialOAuthCallback(undefined, button.mode, button.queryDefaults)}
                  >
                    {t(button.labelKey)}
                  </Button>
                </div>
              </div>
            );
          })}
        </div>
      ) : null}
      {oauthRawResult ? (
        <div className="space-y-2 rounded-lg border border-border p-3">
          <div className="text-xs font-semibold uppercase tracking-[0.08em] text-muted">
            {t("providers.oauth.response")}
          </div>
          <TextArea value={oauthRawResult} rows={10} readOnly onChange={() => {}} />
        </div>
      ) : null}
    </div>
  ) : (
    <div className="provider-card text-sm text-muted">{t("providers.oauth.unsupported")}</div>
  );

  return (
    <div className="space-y-4">
      {mode === "bulk" ? (
        <>
          <CredentialBulkSection
            bulkModes={bulkModes}
            bulkMode={bulkMode}
            setBulkMode={setBulkMode}
            bulkInputText={bulkInputText}
            setBulkInputText={setBulkInputText}
            bulkPlaceholder={bulkPlaceholder}
            bulkError={bulkError}
            setBulkError={setBulkError}
            runBulkImport={runBulkImport}
            runBulkExport={runBulkExport}
            openBulkImportFilePicker={openBulkImportFilePicker}
            runBulkExportFile={runBulkExportFile}
            bulkFileInputRef={bulkFileInputRef}
            onBulkImportFileChange={onBulkImportFileChange}
            bulkExportText={bulkExportText}
            t={t}
          />
        </>
      ) : mode === "oauth" ? (
        <>{oauthSection}</>
      ) : (
        <>
          <div className="flex flex-wrap items-end gap-2">
            <div className="w-24">
              <Select
                value={credentialSearchMode}
                onChange={(value) => onChangeCredentialSearchMode(value as CredentialSearchMode)}
                options={[
                  { value: "id", label: t("providers.search.mode.id") },
                  { value: "name", label: t("providers.search.mode.name") }
                ]}
              />
            </div>
            <div className="min-w-[180px] flex-1">
              <Input
                value={credentialSearchText}
                onChange={onChangeCredentialSearchText}
                placeholder={t("providers.search.placeholder.credential")}
              />
            </div>
            <div className="w-20">
              <Select
                value={String(credentialPageSize)}
                onChange={(value) => onChangeCredentialPageSize(Number(value))}
                options={[
                  { value: "5", label: "5" },
                  { value: "10", label: "10" },
                  { value: "20", label: "20" },
                  { value: "50", label: "50" }
                ]}
              />
            </div>
            <Button
              variant="neutral"
              disabled={credentialTotalCount === 0 || testingAllCredentials}
              onClick={onTestAllCredentials}
            >
              {testingAllCredentials
                ? t("providers.credentials.testingAll")
                : t("providers.credentials.testAll")}
            </Button>
            <Button
              variant={deadCredentialCount > 0 ? "danger" : "neutral"}
              disabled={deadCredentialCount === 0 || testingAllCredentials}
              onClick={onRequestRemoveDeadCredentials}
            >
              {t("providers.credentials.deleteDead", { count: deadCredentialCount })}
            </Button>
          </div>
          <div className="text-xs text-muted">
            {t("providers.credentials.summary", {
              total: credentialTotalCount,
              matched: credentialFilteredCount
            })}
          </div>

          {credentialRows.length === 0 ? (
            <div className="provider-card text-sm text-muted">
              {credentialListLoading ? t("common.loading") : t("providers.search.emptyCredential")}
            </div>
          ) : (
            <>
              <CredentialCardsSection
                channel={channel}
                credentialRows={credentialRows}
                statusesByCredential={statusesByCredential}
                usageByCredential={usageByCredential}
                liveUsageRowsByCredential={liveUsageRowsByCredential}
                usageDisplayKindByCredential={usageDisplayKindByCredential}
                usageDisplayRowsByCredential={usageDisplayRowsByCredential}
                usageLoadingByCredential={usageLoadingByCredential}
                usageErrorByCredential={usageErrorByCredential}
                supportsUpstreamUsage={supportsUpstreamUsage}
                expandedCooldownCredentialId={expandedCooldownCredentialId}
                setExpandedCooldownCredentialId={setExpandedCooldownCredentialId}
                selectedCooldownKeysByCredential={selectedCooldownKeysByCredential}
                setSelectedCooldownKeysByCredential={setSelectedCooldownKeysByCredential}
                statusEditorCredentialId={statusEditorCredentialId}
                setStatusEditorCredentialId={setStatusEditorCredentialId}
                statusForm={statusForm}
                setStatusForm={setStatusForm}
                onEditCredential={(row) => {
                  setListEditorCredentialId(row.id);
                  onEditCredential(row);
                }}
                onCopyCredential={onCopyCredential}
                onRemoveCredential={onRemoveCredential}
                onToggleCredentialEnabled={onToggleCredentialEnabled}
                onSetCredentialHealth={onSetCredentialHealth}
                onQueryUpstreamUsage={onQueryUpstreamUsage}
                onUpsertStatus={onUpsertStatus}
                normalizeHealthKind={normalizeHealthKind}
                parseCooldowns={parseCooldowns}
                healthLabel={healthLabel}
                cooldownKey={cooldownKey}
                formatWindowLabel={formatWindowLabel}
                resolveUsageGroupLabel={resolveUsageGroupLabel}
                resolveLiveLimitLabel={resolveLiveLimitLabel}
                t={t}
              />
              <div className="flex flex-wrap items-center justify-between gap-2 text-xs text-muted">
                <div>
                  {t("providers.pager.statsDetailed", {
                    shown: credentialRows.length,
                    filtered: credentialFilteredCount,
                    total: credentialTotalCount
                  })}
                </div>
                <div className="flex items-center gap-2">
                  <Button
                    variant="neutral"
                    disabled={credentialPage <= 1 || credentialListLoading}
                    onClick={() => onChangeCredentialPage(Math.max(1, credentialPage - 1))}
                  >
                    {t("providers.pager.prev")}
                  </Button>
                  <span>
                    {t("providers.pager.page", {
                      current: credentialPage,
                      total: listTotalPages
                    })}
                  </span>
                  <Button
                    variant="neutral"
                    disabled={credentialPage >= listTotalPages || credentialListLoading}
                    onClick={() =>
                      onChangeCredentialPage(Math.min(listTotalPages, credentialPage + 1))
                    }
                  >
                    {t("providers.pager.next")}
                  </Button>
                </div>
              </div>
            </>
          )}

          {listEditorCredentialId !== null ? (
            <div className="provider-card space-y-3">
              <div className="flex flex-wrap items-center justify-between gap-2">
                <div className="text-xs font-semibold uppercase tracking-[0.08em] text-muted">
                  {t("providers.subtab.single")} #{listEditorCredentialId}
                </div>
                <Button variant="neutral" onClick={() => setListEditorCredentialId(null)}>
                  {t("common.close")}
                </Button>
              </div>
              <CredentialSingleSection
                credentialForm={credentialForm}
                setCredentialForm={setCredentialForm}
                credentialSchema={credentialSchema}
                renderCredentialField={renderCredentialField}
                onUpsertCredential={onUpsertCredential}
                t={t}
              />
            </div>
          ) : null}
        </>
      )}
    </div>
  );
}
