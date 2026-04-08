import { useEffect, useState } from "react";

import { Button, Card, Input, Label, Select, TextArea } from "../../../components/ui";
import {
  DISPATCH_IMPLEMENTATION_OPTIONS,
  DISPATCH_OPERATION_OPTIONS,
  DISPATCH_PROTOCOL_OPTIONS,
  createDispatchRuleDraft,
} from "./dispatch";
import { settingsFieldsForChannel } from "./channel-forms";
import type { ProviderFormState } from "./index";

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
    id: string;
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

  return (
    <Card title={labels.subtitle}>
      <div className="grid gap-4 lg:grid-cols-2">
        <div>
          <Label>{labels.id}</Label>
          <Input value={form.id} onChange={(value) => onChange({ id: value })} />
        </div>
        <div>
          <Label>{labels.name}</Label>
          <Input value={form.name} onChange={(value) => onChange({ name: value })} />
        </div>
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
      <div className="mt-4 grid gap-4 lg:grid-cols-2">
        {settingsFieldsForChannel(form.channel).map((field) => (
          <div key={field.key}>
            <Label>{field.label}</Label>
            {field.type === "textarea" ? (
              <TextArea
                value={form.settings[field.key] ?? ""}
                onChange={(value) =>
                  onChange({ settings: { ...form.settings, [field.key]: value } })
                }
                rows={4}
              />
            ) : field.type === "boolean" ? (
              <Select
                value={form.settings[field.key] ?? "false"}
                onChange={(value) =>
                  onChange({ settings: { ...form.settings, [field.key]: value } })
                }
                options={[
                  { value: "false", label: "false" },
                  { value: "true", label: "true" },
                ]}
              />
            ) : (
              <Input
                value={form.settings[field.key] ?? ""}
                onChange={(value) =>
                  onChange({ settings: { ...form.settings, [field.key]: value } })
                }
              />
            )}
          </div>
        ))}
      </div>

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

        {dispatchExpanded ? (
          <div className="space-y-3">
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

                <div className="grid gap-4 lg:grid-cols-3">
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
                  <div className="grid gap-4 lg:grid-cols-2">
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
