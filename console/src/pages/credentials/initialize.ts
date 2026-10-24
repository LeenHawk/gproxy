import { toast } from "sonner"
import i18n from "@/i18n"
import { refreshCredential } from "@/api/credentials"
import type { CredentialDto } from "@/generated/sdk"

export async function initializeCredentials(rows: Array<CredentialDto | null>) {
  const pending = rows.filter((row): row is CredentialDto => row !== null && row.expiresAtMs === 0)
  const results = await Promise.allSettled(pending.map(row => refreshCredential(row.id, false)))
  for (const [index, result] of results.entries()) {
    if (result.status === "rejected") {
      toast.error(i18n.t("management.savedRefreshFailed", { label: pending[index].label ?? pending[index].id }), {
        description: result.reason instanceof Error ? result.reason.message : String(result.reason),
      })
    }
  }
}
