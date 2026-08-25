import { useTranslation } from "react-i18next"
import type { QuotaWindowDto } from "@/generated/QuotaWindowDto"
import type { UserDto } from "@/generated/UserDto"
import type { UserKeyDto } from "@/generated/UserKeyDto"
import { WindowBar } from "@/components/window-bar"

function windowLabel(kind: string, t: (key: string) => string) {
  if (kind === "total") return t("access.quotas.total")
  if (kind === "daily") return t("access.quotas.daily")
  if (kind === "weekly") return t("access.quotas.weekly")
  if (kind === "monthly") return t("access.quotas.monthly")
  if (kind === "five_hour") return t("access.quotas.fiveHour")
  if (kind === "seven_day") return t("access.quotas.sevenDay")
  return kind
}

export function QuotaWindowBar({ window, users, keys }: { window: QuotaWindowDto; users?: Array<UserDto>; keys?: Array<UserKeyDto> }) {
  const { t } = useTranslation()
  const subjectKind = window.subject_kind === "user"
    ? t("access.subjectKinds.user")
    : window.subject_kind === "user_key" ? t("access.subjectKinds.userKey") : window.subject_kind
  const user = window.subject_kind === "user" ? users?.find((item) => item.id === window.subject_id) : undefined
  const key = window.subject_kind === "user_key" ? keys?.find((item) => item.id === window.subject_id) : undefined
  const subject = user?.name ?? key?.label ?? (key ? t("users.keys.masked", { prefix: key.prefix ?? "" }) : `#${window.subject_id}`)

  return (
    <WindowBar
      label={`${subjectKind} · ${subject} · ${windowLabel(window.window_kind, t)}`}
      used={window.cost_used}
      limit={window.cost_limit}
      start={window.window_start}
      end={window.reset_at}
      started={window.started}
    />
  )
}
