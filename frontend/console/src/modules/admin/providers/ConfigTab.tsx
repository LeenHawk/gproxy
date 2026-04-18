import { useEffect, useState } from "react";

import { useI18n } from "../../../app/i18n";
import { Button, Card, Input, Label, Select, StatusToggle, TextArea } from "../../../components/ui";
import {
  ROUTING_IMPLEMENTATION_OPTIONS,
  ROUTING_OPERATION_OPTIONS,
  ROUTING_PROTOCOL_OPTIONS,
  ROUTING_TEMPLATES,
  applyRoutingTemplate,
  createRoutingRuleDraft,
  isRoutingTemplateMatch,
} from "./routing";
import { settingsFieldsForChannel } from "./channel-forms";
import type { ProviderFormState } from "./index";
import {
  BetaHeadersEditor,
  CacheBreakpointsEditor,
  PreludeTextEditor,
  SanitizeRulesEditor,
} from "./settings-editors";

/// Fields rendered by dedicated editors instead of generic input/textarea.
const EDITOR_FIELDS = new Set([
  "cache_breakpoints",
  "extra_beta_headers",
  "prelude_text",
  "sanitize_rules",
  "rewrite_rules",
]);

/// Channels that show the Anthropic-specific editors (cache breakpoints,
/// beta headers). claudecode additionally gets the prelude editor.
const ANTHROPIC_CHANNELS = new Set(["anthropic", "claudecode"]);

export function ConfigTab({
  form,
  onChange,
  onSave,
  onDelete,
  onRestoreDefault,
  channelOptions,
  labels,
  canDelete,
}: {
  form: ProviderFormState;
  onChange: (patch: Partial<ProviderFormState>) => void;
  onSave: () => void;
  onDelete: () => void;
  onRestoreDefault: () => void;
  channelOptions: Array<{ value: string; label: string }>;
  labels: {
    subtitle: string;
    name: string;
    label: string;
    labelPlaceholder: string;
    channel: string;
    routingRules: string;
    routingHint: string;
    routingRule: string;
    routingSourceOperation: string;
    routingSourceProtocol: string;
    routingMode: string;
    routingDestinationOperation: string;
    routingDestinationProtocol: string;
    routingAddRule: string;
    routingRemoveRule: string;
    routingExpand: string;
    routingCollapse: string;
    routingCollapsedSummary: string;
    modePassthrough: string;
    modeTransformTo: string;
    modeLocal: string;
    modeUnsupported: string;
    save: string;
    delete: string;
    newHint: string;
  };
  canDelete: boolean;
}) {
  const { t } = useI18n();
  const [routingExpanded, setRoutingExpanded] = useState(false);
  const [templatesExpanded, setTemplatesExpanded] = useState(false);
  const modeOptions = ROUTING_IMPLEMENTATION_OPTIONS.map((option) => ({
    value: option.value,
    label:
      option.value === "Passthrough"
        ? labels.modePassthrough
        : option.value === "TransformTo"
          ? labels.modeTransformTo
          : option.value === "Local"
            ? labels.modeLocal
            : labels.modeUnsupported,
  }));

  useEffect(() => {
    setRoutingExpanded(false);
    setTemplatesExpanded(false);
  }, [form.id, form.channel]);

  const updateSetting = (key: string, value: string) => {
    onChange({ settings: { ...form.settings, [key]: value } });
  };

  const isAnthropic = ANTHROPIC_CHANNELS.has(form.channel);
  const isClaudeCode = form.channel === "claudecode";

  const fieldLabel = (field: { key: string; label: string }) => {
    const i18nKey = "field." + field.key;
    const translated = t(i18nKey);
    return translated !== i18nKey ? translated : field.label;
  };

  // Filter out fields handled by dedicated editors
  const genericFields = settingsFieldsForChannel(form.channel).filter(
    (field) => !EDITOR_FIELDS.has(field.key),
  );

  return (
    <Card title={labels.subtitle}>
      <div>
        <Label>{labels.name}</Label>
        <Input value={form.name} onChange={(value) => onChange({ name: value })} />
      </div>
      <div className="mt-4">
        <Label>{labels.label}</Label>
        <Input
          value={form.label}
          onChange={(value) => onChange({ label: value })}
          placeholder={labels.labelPlaceholder}
        />
      </div>
      <div className="mt-4">
        <Label>{labels.channel}</Label>
        <Select
          value={form.channel}
          disabled={canDelete}
          onChange={(value) => onChange({ channel: value, settings: {} })}
          options={channelOptions}
        />
        {!canDelete ? <p className="mt-2 text-xs text-muted">{labels.newHint}</p> : null}
      </div>

      {/* Generic fields (base_url, user_agent, oauth URLs, etc.) */}
      <div className="mt-4 grid gap-4 md:grid-cols-2">
        {genericFields.map((field) => (
          <div key={field.key}>
            {field.type === "boolean" ? (
              <StatusToggle
                label={fieldLabel(field)}
                checked={form.settings[field.key] === "true"}
                onToggle={() =>
                  updateSetting(
                    field.key,
                    form.settings[field.key] === "true" ? "false" : "true",
                  )
                }
                checkedLabel={t("common.enabled")}
                uncheckedLabel={t("common.disabled")}
              />
            ) : (
              <>
                <Label>{fieldLabel(field)}</Label>
                {field.type === "textarea" || field.type === "json" ? (
                  <TextArea
                    value={form.settings[field.key] ?? ""}
                    onChange={(value) => updateSetting(field.key, value)}
                    rows={field.type === "json" ? 6 : 4}
                  />
                ) : (
                  <Input
                    value={form.settings[field.key] ?? ""}
                    onChange={(value) => updateSetting(field.key, value)}
                  />
                )}
              </>
            )}
          </div>
        ))}
      </div>

      {/* Anthropic-specific: cache breakpoints */}
      {isAnthropic ? (
        <div className="mt-6">
          <CacheBreakpointsEditor
            value={form.settings.cache_breakpoints ?? "[]"}
            onChange={(v) => updateSetting("cache_breakpoints", v)}
            t={t}
          />
        </div>
      ) : null}

      {/* Anthropic-specific: beta headers */}
      {isAnthropic ? (
        <div className="mt-6">
          <BetaHeadersEditor
            value={form.settings.extra_beta_headers ?? "[]"}
            onChange={(v) => updateSetting("extra_beta_headers", v)}
            isClaudeCode={isClaudeCode}
            t={t}
          />
        </div>
      ) : null}

      {/* ClaudeCode-specific: prelude text */}
      {isClaudeCode ? (
        <div className="mt-6">
          <PreludeTextEditor
            value={form.settings.prelude_text ?? ""}
            onChange={(v) => updateSetting("prelude_text", v)}
            t={t}
          />
        </div>
      ) : null}

      {/* All channels: message rewrite rules */}
      <div className="mt-6">
        <SanitizeRulesEditor
          value={form.settings.sanitize_rules ?? "[]"}
          onChange={(v) => updateSetting("sanitize_rules", v)}
          t={t}
        />
      </div>

      {/* Routing rules */}
      <div className="panel-shell mt-6 space-y-4">
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div>
            <div className="text-sm font-semibold text-text">{labels.routingRules}</div>
            <p className="mt-1 text-xs text-muted">{labels.routingHint}</p>
          </div>
          <div className="flex flex-wrap gap-2">
            <Button
              variant="neutral"
              onClick={() => {
                onRestoreDefault();
                setRoutingExpanded(true);
              }}
            >
              {t("providers.routing.restoreDefault")}
            </Button>
            <Button variant="neutral" onClick={() => setRoutingExpanded((value) => !value)}>
              {routingExpanded ? labels.routingCollapse : labels.routingExpand}
            </Button>
            {routingExpanded ? (
              <Button
                variant="neutral"
                onClick={() =>
                  onChange({ routingRules: [...form.routingRules, createRoutingRuleDraft()] })
                }
              >
                {labels.routingAddRule}
              </Button>
            ) : null}
          </div>
        </div>

        {/* Template chips */}
        <div>
            <div className="mb-1.5 flex items-center justify-between gap-2">
              <div className="text-xs text-muted">{t("providers.routing.templates")}</div>
              <button
                type="button"
                className="btn btn-neutral rounded-full px-2.5 py-1 text-[11px] font-semibold"
                onClick={() => setTemplatesExpanded((value) => !value)}
              >
                {templatesExpanded
                  ? t("providers.routing.templatesCollapse")
                  : t("providers.routing.templatesExpand")}
              </button>
            </div>
            <p className="mb-2 text-[11px] text-muted">{t("providers.routing.templatesHint")}</p>
            {templatesExpanded ? (
              <div className="flex flex-wrap gap-1.5">
                {ROUTING_TEMPLATES.map((tmpl) => {
                  const active = isRoutingTemplateMatch(tmpl, form.routingRules);
                  return (
                    <button
                      key={tmpl.key}
                      type="button"
                      className={`btn rounded-full px-2.5 py-1 text-[11px] font-semibold transition ${
                        active ? "btn-primary" : "btn-neutral"
                      }`}
                      onClick={() => {
                        onChange({ routingRules: applyRoutingTemplate(tmpl) });
                        setRoutingExpanded(true);
                      }}
                    >
                      {tmpl.label}
                    </button>
                  );
                })}
              </div>
            ) : null}
          </div>

        {routingExpanded ? (
          <div className="max-h-128 space-y-3 overflow-y-auto pr-1">
            {form.routingRules.map((rule, index) => (
              <div key={rule.id} className="panel-shell panel-shell-compact space-y-4">
                <div className="flex items-center justify-between gap-3">
                  <div className="text-sm font-semibold text-text">
                    {labels.routingRule} {index + 1}
                  </div>
                  <Button
                    variant="danger"
                    disabled={form.routingRules.length === 1}
                    onClick={() =>
                      onChange({
                        routingRules: form.routingRules.filter((item) => item.id !== rule.id),
                      })
                    }
                  >
                    {labels.routingRemoveRule}
                  </Button>
                </div>

                <div className="grid gap-4 md:grid-cols-3">
                  <div>
                    <Label>{labels.routingSourceOperation}</Label>
                    <Select
                      value={rule.srcOperation}
                      onChange={(value) =>
                        onChange({
                          routingRules: form.routingRules.map((item) =>
                            item.id === rule.id ? { ...item, srcOperation: value } : item,
                          ),
                        })
                      }
                      options={ROUTING_OPERATION_OPTIONS}
                    />
                  </div>
                  <div>
                    <Label>{labels.routingSourceProtocol}</Label>
                    <Select
                      value={rule.srcProtocol}
                      onChange={(value) =>
                        onChange({
                          routingRules: form.routingRules.map((item) =>
                            item.id === rule.id ? { ...item, srcProtocol: value } : item,
                          ),
                        })
                      }
                      options={ROUTING_PROTOCOL_OPTIONS}
                    />
                  </div>
                  <div>
                    <Label>{labels.routingMode}</Label>
                    <Select
                      value={rule.implementation}
                      onChange={(value) =>
                        onChange({
                          routingRules: form.routingRules.map((item) =>
                            item.id === rule.id
                              ? {
                                  ...item,
                                  implementation: value as typeof item.implementation,
                                  destinationOperation:
                                    value === "TransformTo"
                                      ? item.destinationOperation || item.srcOperation
                                      : "",
                                  destinationProtocol:
                                    value === "TransformTo"
                                      ? item.destinationProtocol || item.srcProtocol
                                      : "",
                                }
                              : item,
                          ),
                        })
                      }
                      options={modeOptions}
                    />
                  </div>
                </div>

                {rule.implementation === "TransformTo" ? (
                  <div className="grid gap-4 md:grid-cols-2">
                    <div>
                      <Label>{labels.routingDestinationOperation}</Label>
                      <Select
                        value={rule.destinationOperation}
                        onChange={(value) =>
                          onChange({
                            routingRules: form.routingRules.map((item) =>
                              item.id === rule.id ? { ...item, destinationOperation: value } : item,
                            ),
                          })
                        }
                        options={ROUTING_OPERATION_OPTIONS}
                      />
                    </div>
                    <div>
                      <Label>{labels.routingDestinationProtocol}</Label>
                      <Select
                        value={rule.destinationProtocol}
                        onChange={(value) =>
                          onChange({
                            routingRules: form.routingRules.map((item) =>
                              item.id === rule.id ? { ...item, destinationProtocol: value } : item,
                            ),
                          })
                        }
                        options={ROUTING_PROTOCOL_OPTIONS}
                      />
                    </div>
                  </div>
                ) : null}
              </div>
            ))}
          </div>
        ) : (
          <div className="text-sm text-muted">
            {labels.routingCollapsedSummary.replace("{count}", String(form.routingRules.length))}
          </div>
        )}
      </div>

      <div className="mt-4 flex gap-2">
        <Button onClick={onSave}>{labels.save}</Button>
        {canDelete ? (
          <Button variant="danger" onClick={onDelete}>
            {labels.delete}
          </Button>
        ) : null}
      </div>
    </Card>
  );
}
