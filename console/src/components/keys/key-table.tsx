import { useMemo } from "react"
import { useTranslation } from "react-i18next"
import type { UserDto } from "@/generated/UserDto"
import type { UserKeyDto } from "@/generated/UserKeyDto"
import type { UserKeyRevealResponse } from "@/generated/UserKeyRevealResponse"
import { KeySecretCell } from "@/components/keys/key-secret-cell"
import { Switch } from "@/components/ui/switch"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
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
  return (
    <div className="overflow-hidden rounded-md border bg-card">
      <Table>
        <TableHeader><TableRow>
          <TableHead>{t("users.keys.label")}</TableHead>
          <TableHead>{t("access.subjectKinds.user")}</TableHead>
          <TableHead>{t("users.keys.title")}</TableHead>
          <TableHead>{t("users.keys.expiresAt")}</TableHead>
          <TableHead>{t("common.status.label")}</TableHead>
        </TableRow></TableHeader>
        <TableBody>{props.keys.map((key) => {
          const label = key.label ?? key.prefix ?? String(key.id)
          const action = t(key.enabled ? "common.actions.disable" : "common.actions.enable")
          return (
            <TableRow key={key.id}>
              <TableCell>{key.label ?? t("common.none")}</TableCell>
              <TableCell>{userNames.get(key.user_id) ?? key.user_id}</TableCell>
              <TableCell><KeySecretCell record={key} reveal={() => props.reveal(key.id)} /></TableCell>
              <TableCell>{formatInstant(key.expires_at, i18n.language) ?? t("users.keys.neverExpires")}</TableCell>
              <TableCell><Switch checked={key.enabled} disabled={props.pending} aria-label={`${action} ${label}`} onCheckedChange={() => props.onEnabledChange(key)} /></TableCell>
            </TableRow>
          )
        })}</TableBody>
      </Table>
    </div>
  )
}
