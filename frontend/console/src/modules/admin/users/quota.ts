import { parseOptionalFloat } from "../../../lib/form";
import type { MemoryUserQuotaRow, UserQuotaWrite } from "../../../lib/types/admin";

export type UserQuotaFormState = {
  quota: string;
  cost_used: string;
};

export function buildUserQuotaFormState(
  row?: Pick<MemoryUserQuotaRow, "quota" | "cost_used"> | null,
): UserQuotaFormState {
  return {
    quota: row ? String(row.quota) : "",
    cost_used: row ? String(row.cost_used) : "",
  };
}

export function buildUserQuotaWritePayload(userId: number, form: UserQuotaFormState): UserQuotaWrite {
  return {
    user_id: userId,
    quota: parseOptionalFloat(form.quota) ?? 0,
    cost_used: parseOptionalFloat(form.cost_used) ?? 0,
  };
}
