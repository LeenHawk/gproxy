import type { Dispatch, ReactNode, SetStateAction } from "react";

import { Button, Input, Label } from "../../../../components/ui";
import type {
  ChannelCredentialSchema,
  CredentialFieldSchema,
  CredentialFormState
} from "../index";
import type { TranslateFn } from "./shared";

export function CredentialSingleSection({
  credentialForm,
  setCredentialForm,
  credentialSchema,
  renderCredentialField,
  onUpsertCredential,
  extraSectionBeforeOAuth,
  t
}: {
  credentialForm: CredentialFormState;
  setCredentialForm: Dispatch<SetStateAction<CredentialFormState>>;
  credentialSchema: ChannelCredentialSchema;
  renderCredentialField: (field: CredentialFieldSchema) => ReactNode;
  onUpsertCredential: () => void;
  extraSectionBeforeOAuth?: ReactNode;
  t: TranslateFn;
}) {
  return (
    <>
      <div className="grid gap-3 md:grid-cols-2">
        <div>
          <Label>{t("field.id")}</Label>
          <Input value={credentialForm.id} onChange={() => {}} disabled />
        </div>
        <div>
          <Label>{t("field.nameOptional")}</Label>
          <Input
            value={credentialForm.name}
            onChange={(v) => setCredentialForm((p) => ({ ...p, name: v }))}
          />
        </div>
        {credentialSchema.fields.map((field) => renderCredentialField(field))}
      </div>

      <div>
        <Button onClick={onUpsertCredential}>{t("common.save")}</Button>
      </div>

      {extraSectionBeforeOAuth ?? null}
    </>
  );
}
