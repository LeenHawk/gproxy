import { parseRequiredI64 } from "../../../lib/form";
import type { UserWrite } from "../../../lib/types/admin";

export type UserFormState = {
  id: string;
  name: string;
  password: string;
  enabled: boolean;
  is_admin: boolean;
};

export function buildUserWritePayload(form: UserFormState): UserWrite {
  return {
    id: parseRequiredI64(form.id, "id"),
    name: form.name.trim(),
    password: form.password,
    enabled: form.enabled,
    is_admin: form.is_admin,
  };
}
