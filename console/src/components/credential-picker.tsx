import { useId } from "react"
import { useTranslation } from "react-i18next"
import { credentials } from "@/api/configuration"
import { optionSource } from "@/api/options"
import { SearchableSelect } from "@/components/searchable-select"
export function CredentialPicker({ providerId, value, onChange, disabled }: { providerId: string; value: string | null; onChange: (id: string | null) => void; disabled?: boolean }) {
  const { t } = useTranslation(), id = useId()
  return <SearchableSelect id={id} label={t("management.testCredential")} value={value ?? ""} onChange={id => onChange(id || null)} disabled={disabled} emptyLabel={t("management.autoCredential")} source={optionSource(credentials, row => ({ value: row.id, label: row.label ?? row.id }), { providerId })} />
}
