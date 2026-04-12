import { useEffect, useState } from "react";

import { useI18n } from "../../../app/i18n";
import { Button, Card, Input, Label, Select, TextArea } from "../../../components/ui";
import {
  DISPATCH_IMPLEMENTATION_OPTIONS,
  DISPATCH_OPERATION_OPTIONS,
  DISPATCH_PROTOCOL_OPTIONS,
  DISPATCH_TEMPLATES,
  applyDispatchTemplate,
  createDispatchRuleDraft,
  isDispatchTemplateMatch,
} from "./dispatch";
import { settingsFieldsForChannel } from "./channel-forms";
import type { ProviderFormState } from "./index";
import {
  BetaHeadersEditor,
  CacheBreakpointsEditor,
  PreludeTextEditor,
  SanitizeRulesEditor,
} from "./SettingsEditors";

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
  channelOptions,
  labels,
  canDelete,
}: {
  form: ProviderFormState;
  onChange: (patch: Partial<ProviderFormState>) => void;
  onSave: () => void;
  onDelete: () => void;
  channelOptions: Array<{ value: string; label: string }>;
  labels: {
    subtitle: string;
    name: string;
    channel: string;
    dispatchRules: string;
    dispatchHint: string;
    dispatchRule: string;
    dispatchSourceOperation: string;
    dispatchSourceProtocol: string;
    dispatchMode: string;
    dispatchDestinationOperation: string;
    dispatchDestinationProtocol: string;
    dispatchAddRule: string;
    dispatchRemoveRule: string;
    dispatchExpand: string;
    dispatchCollapse: string;
    dispatchCollapsedSummary: string;
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
  const [dispatchExpanded, setDispatchExpanded] = useState(false);
  const modeOptions = DISPATCH_IMPLEMENTATION_OPTIONS.map((option) => ({
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
    setDispatchExpanded(false);
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
            <Label>{fieldLabel(field)}</Label>
            {field.type === "textarea" || field.type === "json" ? (
              <TextArea
                value={form.settings[field.key] ?? ""}
                onChange={(value) => updateSetting(field.key, value)}
                rows={field.type === "json" ? 6 : 4}
              />
            ) : field.type === "boolean" ? (
              <Select
                value={form.settings[field.key] ?? "false"}
                onChange={(value) => updateSetting(field.key, value)}
                options={[
                  { value: "false", label: "false" },
                  { value: "true", label: "true" },
                ]}
              />
            ) : (
              <Input
                value={form.settings[field.key] ?? ""}
                onChange={(value) => updateSetting(field.key, value)}
              />
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

      {/* Dispatch rules */}
      <div className="panel-shell mt-6 space-y-4">
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div>
            <div className="text-sm font-semibold text-text">{labels.dispatchRules}</div>
            <p className="mt-1 text-xs text-muted">{labels.dispatchHint}</p>
          </div>
          <div className="flex flex-wrap gap-2">
            <Button variant="neutral" onClick={() => setDispatchExpanded((value) => !value)}>
              {dispatchExpanded ? labels.dispatchCollapse : labels.dispatchExpand}
            </Button>
            {dispatchExpanded ? (
              <Button
                variant="neutral"
                onClick={() =>
                  onChange({ dispatchRules: [...form.dispatchRules, createDispatchRuleDraft()] })
                }
              >
                {labels.dispatchAddRule}
              </Button>
            ) : null}
          </div>
        </div>

        {/* Template chips */}
        <div>
            <div className="mb-1.5 text-xs text-muted">{t("providers.dispatch.templates")}</div>
            <div className="flex flex-wrap gap-1.5">
              {DISPATCH_TEMPLATES.map((tmpl) => {
                const active = isDispatchTemplateMatch(tmpl, form.dispatchRules);
                return (
                  <button
                    key={tmpl.key}
                    type="button"
                    className={`btn rounded-full px-2.5 py-1 text-[11px] font-semibold transition ${
                      active ? "btn-primary" : "btn-neutral"
                    }`}
                    onClick={() => {
                      onChange({ dispatchRules: applyDispatchTemplate(tmpl) });
                      setDispatchExpanded(true);
                    }}
                  >
                    {tmpl.label}
                  </button>
                );
              })}
            </div>
          </div>

        {dispatchExpanded ? (
          <div className="max-h-128 space-y-3 overflow-y-auto pr-1">
            {form.dispatchRules.map((rule, index) => (
              <div key={rule.id} className="panel-shell panel-shell-compact space-y-4">
                <div className="flex items-center justify-between gap-3">
                  <div className="text-sm font-semibold text-text">
                    {labels.dispatchRule} {index + 1}
                  </div>
                  <Button
                    variant="danger"
                    disabled={form.dispatchRules.length === 1}
                    onClick={() =>
                      onChange({
                        dispatchRules: form.dispatchRules.filter((item) => item.id !== rule.id),
                      })
                    }
                  >
                    {labels.dispatchRemoveRule}
                  </Button>
                </div>

                <div className="grid gap-4 md:grid-cols-3">
                  <div>
                    <Label>{labels.dispatchSourceOperation}</Label>
                    <Select
                      value={rule.srcOperation}
                      onChange={(value) =>
                        onChange({
                          dispatchRules: form.dispatchRules.map((item) =>
                            item.id === rule.id ? { ...item, srcOperation: value } : item,
                          ),
                        })
                      }
                      options={DISPATCH_OPERATION_OPTIONS}
                    />
                  </div>
                  <div>
                    <Label>{labels.dispatchSourceProtocol}</Label>
                    <Select
                      value={rule.srcProtocol}
                      onChange={(value) =>
                        onChange({
                          dispatchRules: form.dispatchRules.map((item) =>
                            item.id === rule.id ? { ...item, srcProtocol: value } : item,
                          ),
                        })
                      }
                      options={DISPATCH_PROTOCOL_OPTIONS}
                    />
                  </div>
                  <div>
                    <Label>{labels.dispatchMode}</Label>
                    <Select
                      value={rule.implementation}
                      onChange={(value) =>
                        onChange({
                          dispatchRules: form.dispatchRules.map((item) =>
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
                      <Label>{labels.dispatchDestinationOperation}</Label>
                      <Select
                        value={rule.destinationOperation}
                        onChange={(value) =>
                          onChange({
                            dispatchRules: form.dispatchRules.map((item) =>
                              item.id === rule.id ? { ...item, destinationOperation: value } : item,
                            ),
                          })
                        }
                        options={DISPATCH_OPERATION_OPTIONS}
                      />
                    </div>
                    <div>
                      <Label>{labels.dispatchDestinationProtocol}</Label>
                      <Select
                        value={rule.destinationProtocol}
                        onChange={(value) =>
                          onChange({
                            dispatchRules: form.dispatchRules.map((item) =>
                              item.id === rule.id ? { ...item, destinationProtocol: value } : item,
                            ),
                          })
                        }
                        options={DISPATCH_PROTOCOL_OPTIONS}
                      />
                    </div>
                  </div>
                ) : null}
              </div>
            ))}
          </div>
        ) : (
          <div className="text-sm text-muted">
            {labels.dispatchCollapsedSummary.replace("{count}", String(form.dispatchRules.length))}
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
