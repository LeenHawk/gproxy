import { useTranslation } from "react-i18next"
import type { InstanceSettingsDto } from "@/generated/InstanceSettingsDto"
import type { RuntimeSettingFieldDto } from "@/generated/RuntimeSettingFieldDto"
import { FieldDescription } from "@/components/ui/field"

export function RuntimeSettingStatus({ draft, field }: { draft: InstanceSettingsDto; field: RuntimeSettingFieldDto }) {
  const { t } = useTranslation()
  const status = draft.runtime_status
  if (!status) return null
  const override = status.overrides.find((item) => item.field === field)
  const effective = status.effective[field]
  if (!override && JSON.stringify(effective) === JSON.stringify(draft[field])) return null
  const value = field === "log_level" ? status.log_filter
    : Array.isArray(effective) ? effective.join(", ") || t("settings.runtime.none")
    : effective == null ? t(status.effective.inherit_system_proxy ? "settings.runtime.systemProxy" : "settings.runtime.direct") : String(effective)
  return <FieldDescription className="break-all">{t(override ? "settings.runtime.overridden" : "settings.runtime.pending", { value, source: override?.source })}</FieldDescription>
}
