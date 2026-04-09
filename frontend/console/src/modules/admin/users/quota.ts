import { parseOptionalFloat } from "../../../lib/form";
import type { MemoryUserQuotaRow, UserQuotaWrite } from "../../../lib/types/admin";

export function buildQuotaIncrementPayload(
  quota: Pick<MemoryUserQuotaRow, "user_id" | "quota" | "cost_used">,
  increment: string | number,
): UserQuotaWrite {
  const parsedIncrement =
    typeof increment === "number" ? increment : (parseOptionalFloat(increment) ?? 0);
  return {
    user_id: quota.user_id,
    quota: quota.quota + parsedIncrement,
    cost_used: quota.cost_used,
  };
}
