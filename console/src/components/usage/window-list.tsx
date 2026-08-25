import { useTranslation } from "react-i18next"
import type { CredentialQuotaCycleDto } from "@/generated/CredentialQuotaCycleDto"
import type { QuotaWindowDto } from "@/generated/QuotaWindowDto"
import type { UserDto } from "@/generated/UserDto"
import type { UserKeyDto } from "@/generated/UserKeyDto"
import { CycleWindow } from "@/components/cycle-window"
import { QuotaWindowBar } from "@/components/usage/quota-window"

export function WindowList({ quotas, cycles, users, keys }: { quotas: Array<QuotaWindowDto>; cycles: Array<CredentialQuotaCycleDto>; users: Array<UserDto>; keys: Array<UserKeyDto> }) {
  const { t } = useTranslation()
  return (
    <div className="grid gap-8 lg:grid-cols-2">
      <section className="flex flex-col gap-5">
        <h2 className="text-sm font-semibold">{t("usage.quotaWindows")}</h2>
        {quotas.length ? quotas.map((window) => <QuotaWindowBar key={`${window.quota_id}-${window.window_kind}`} window={window} users={users} keys={keys} />) : <p className="text-sm text-muted-foreground">{t("usage.windowStates.empty")}</p>}
      </section>
      <section className="flex flex-col gap-5">
        <h2 className="text-sm font-semibold">{t("usage.credentialCycles")}</h2>
        {cycles.length ? cycles.map((cycle) => <CycleWindow key={cycle.id} cycle={cycle} />) : <p className="text-sm text-muted-foreground">{t("usage.cycleStates.empty")}</p>}
      </section>
    </div>
  )
}
