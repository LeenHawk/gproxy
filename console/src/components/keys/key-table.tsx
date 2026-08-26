import { useMemo } from "react"
import { useTranslation } from "react-i18next"
import type { UserDto } from "@/generated/UserDto"
import type { UserKeyDto } from "@/generated/UserKeyDto"
import type { UserKeyRevealResponse } from "@/generated/UserKeyRevealResponse"
import { DataTable, type DataTableColumn } from "@/components/data-table"
import { KeySecretCell } from "@/components/keys/key-secret-cell"
import { Switch } from "@/components/ui/switch"
import { formatInstant } from "@/lib/format"

type KeyTableProps = {
  keys: Array<UserKeyDto>
  users: Array<UserDto>
  pending: boolean
  reveal: (id: number) => Promise<UserKeyRevealResponse>
  onEnabledChange: (key: UserKeyDto) => void
}

export function KeyTable(props: KeyTableProps) {
  const { t, i18n } = useTranslation()
  const userNames = useMemo(() => new Map(props.users.map((user) => [user.id, user.name])), [props.users])
  const toggle = (key: UserKeyDto) => {
    const label = key.label ?? key.prefix ?? String(key.id)
    const action = t(key.enabled ? "common.actions.disable" : "common.actions.enable")
    return <Switch checked={key.enabled} disabled={props.pending} aria-label={`${action} ${label}`} onCheckedChange={() => props.onEnabledChange(key)} />
  }
  const columns: Array<DataTableColumn<UserKeyDto>> = [
    { key: "label", label: t("users.keys.label"), header: t("users.keys.label"), cell: (key) => key.label ?? t("common.none") },
    { key: "user", label: t("access.subjectKinds.user"), header: t("access.subjectKinds.user"), cell: (key) => userNames.get(key.user_id) ?? key.user_id },
    { key: "secret", label: t("users.keys.title"), header: t("users.keys.title"), cell: (key) => <KeySecretCell record={key} reveal={() => props.reveal(key.id)} /> },
    { key: "expires", label: t("users.keys.expiresAt"), header: t("users.keys.expiresAt"), cell: (key) => formatInstant(key.expires_at, i18n.language) ?? t("users.keys.neverExpires") },
    { key: "status", label: t("common.status.label"), header: t("common.status.label"), cell: toggle },
  ]
  return (
    <DataTable
      columns={columns}
      rows={props.keys}
      rowKey={(key) => key.id}
      searchText={(key) => `${key.label ?? ""} ${key.prefix ?? ""} ${userNames.get(key.user_id) ?? key.user_id}`}
      renderCard={(key) => <div className="flex flex-col gap-3"><div className="flex items-start justify-between gap-3"><div><p className="font-medium">{key.label ?? t("common.none")}</p><p className="text-xs text-muted-foreground">{userNames.get(key.user_id) ?? key.user_id}</p></div>{toggle(key)}</div><KeySecretCell record={key} reveal={() => props.reveal(key.id)} /><p className="text-xs text-muted-foreground">{formatInstant(key.expires_at, i18n.language) ?? t("users.keys.neverExpires")}</p></div>}
      empty={t("users.keys.empty")}
      storageKey="user-keys"
      selectable
    />
  )
}
