import { useQuery } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { credentials } from "@/api/configuration"
import { directory } from "@/api/models"
import { ErrorNotice } from "@/components/state"
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
export function CredentialPicker({ providerId, value, onChange, disabled }: { providerId: string; value: string | null; onChange: (id: string | null) => void; disabled?: boolean }) {
  const { t } = useTranslation()
  const list = useQuery({ queryKey: ["admin", "/credentials", "directory", providerId], queryFn: () => directory(credentials, { providerId }) })
  return <div className="flex flex-col gap-2"><Select value={value ?? "__auto"} onValueChange={id => onChange(id === "__auto" ? null : id)} disabled={disabled || list.isPending}><SelectTrigger aria-label={t("management.testCredential")}><SelectValue /></SelectTrigger><SelectContent><SelectGroup><SelectItem value="__auto">{t("management.autoCredential")}</SelectItem>{list.data?.map(row => <SelectItem key={row.id} value={row.id}>{row.label ?? row.id}</SelectItem>)}</SelectGroup></SelectContent></Select>{list.error ? <ErrorNotice error={list.error} /> : null}</div>
}
