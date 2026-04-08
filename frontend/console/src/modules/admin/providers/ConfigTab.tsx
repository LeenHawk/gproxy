import { Button, Card, Input, Label, Select, TextArea } from "../../../components/ui";
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
    dispatchJson: string;
    save: string;
    delete: string;
    newHint: string;
  };
  canDelete: boolean;
}) {
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
      <div className="mt-4">
        <Label>{labels.dispatchJson}</Label>
        <TextArea
          value={form.dispatchJson}
          onChange={(value) => onChange({ dispatchJson: value })}
          rows={8}
        />
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
