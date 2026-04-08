import { Button, Card, Input, Label, TextArea } from "../../../components/ui";
import type { CredentialRow } from "../../../lib/types/admin";
import { credentialFieldsForChannel } from "./channel-forms";
import type { CredentialFormState } from "./index";

export function CredentialsTab({
  channel,
  credentials,
  form,
  onChangeForm,
  onEdit,
  onDelete,
  onSave,
  labels,
}: {
  channel: string;
  credentials: CredentialRow[];
  form: CredentialFormState;
  onChangeForm: (patch: CredentialFormState) => void;
  onEdit: (row: CredentialRow) => void;
  onDelete: (row: CredentialRow) => void;
  onSave: () => void;
  labels: {
    title: string;
    add: string;
    replace: string;
    none: string;
    edit: string;
    delete: string;
  };
}) {
  const fields = credentialFieldsForChannel(channel);

  return (
    <div className="grid gap-4 lg:grid-cols-[1.1fr_0.9fr]">
      <Card title={labels.title}>
        <div className="space-y-2">
          {credentials.length === 0 ? <p className="text-sm text-muted">{labels.none}</p> : null}
          {credentials.map((row) => (
            <div key={`${row.provider}-${row.index}`} className="card-shell">
              <div className="flex items-start justify-between gap-3">
                <div>
                  <div className="font-semibold">#{row.index}</div>
                  <pre className="mt-2 overflow-auto text-xs text-muted">
                    {JSON.stringify(row.credential, null, 2)}
                  </pre>
                </div>
                <div className="flex gap-2">
                  <Button variant="neutral" onClick={() => onEdit(row)}>
                    {labels.edit}
                  </Button>
                  <Button variant="danger" onClick={() => onDelete(row)}>
                    {labels.delete}
                  </Button>
                </div>
              </div>
            </div>
          ))}
        </div>
      </Card>
      <Card title={form.editingIndex === null ? labels.add : labels.replace}>
        <div className="space-y-4">
          {fields.map((field) => (
            <div key={field.key}>
              <Label>{field.label}</Label>
              {field.type === "textarea" ? (
                <TextArea
                  value={form.values[field.key] ?? ""}
                  onChange={(value) =>
                    onChangeForm({
                      ...form,
                      values: { ...form.values, [field.key]: value },
                    })
                  }
                  rows={4}
                />
              ) : (
                <Input
                  value={form.values[field.key] ?? ""}
                  onChange={(value) =>
                    onChangeForm({
                      ...form,
                      values: { ...form.values, [field.key]: value },
                    })
                  }
                />
              )}
            </div>
          ))}
          <Button onClick={onSave}>
            {form.editingIndex === null ? labels.add : labels.replace}
          </Button>
        </div>
      </Card>
    </div>
  );
}
