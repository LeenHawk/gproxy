import { useMemo } from "react"
import { PlusIcon, UsersIcon } from "lucide-react"
import { useTranslation } from "react-i18next"
import type { UserDto } from "@/generated/UserDto"
import type { UserWriteRequest } from "@/generated/UserWriteRequest"
import type { AccessManagerProps } from "@/components/keys/access-manager"
import { BatchActions } from "@/components/batch-actions"
import { EntityDeleteButton } from "@/components/entity-delete-button"
import { UserDetail, type UserPanel } from "@/components/identity/user-detail"
import { Button } from "@/components/ui/button"
import { Empty, EmptyHeader, EmptyMedia, EmptyTitle } from "@/components/ui/empty"
import { Switch } from "@/components/ui/switch"
import { WorkspaceLayout } from "@/components/workspace/workspace-layout"

export function UserWorkspace({ access, selectedId, panel, pending, onSelect, onBack, onPanel, onCreate, onToggle, onSave }: {
  access: AccessManagerProps
  selectedId: number | null
  panel: UserPanel
  pending: boolean
  onSelect: (user: UserDto) => void
  onBack: () => void
  onPanel: (panel: UserPanel) => void
  onCreate: () => void
  onToggle: (user: UserDto) => void
  onSave: (user: UserDto, value: UserWriteRequest, password: string) => Promise<void>
}) {
  const { t } = useTranslation()
  const organizationNames = useMemo(() => new Map(access.organizations.map((value) => [value.id, value.name])), [access.organizations])
  const teamNames = useMemo(() => new Map(access.teams.map((value) => [value.id, value.name])), [access.teams])
  const selected = access.users.find((user) => user.id === selectedId)
  const summary = (user: UserDto) => [user.organization_id == null ? null : organizationNames.get(user.organization_id), user.team_id == null ? null : teamNames.get(user.team_id), user.is_admin ? t("users.roles.admin") : null].filter(Boolean).join(" · ") || t("common.none")
  return (
    <WorkspaceLayout storageKey="identity-users" title={t("access.subjectKinds.user")} items={access.users} selectedId={selected?.id ?? null} getSearchText={(user) => `${user.name} ${summary(user)}`} renderTitle={(user) => user.name} renderSummary={summary} renderAction={(user) => <div className="flex items-center gap-1"><Switch checked={user.enabled} disabled={pending} aria-label={`${t(user.enabled ? "common.actions.disable" : "common.actions.enable")} ${user.name}`} onCheckedChange={() => onToggle(user)} /><EntityDeleteButton entity="users" id={user.id} label={user.name} queryKeys={["users", "user-keys"]} onDeleted={user.id === selected?.id ? onBack : undefined} /></div>} onSelect={onSelect} onBack={onBack} searchPlaceholder={t("users.searchEntity", { entity: t("access.subjectKinds.user") })} emptyLabel={t("users.empty")} resizeLabel={t("nav.resize")} selectAllLabel={t("common.dataTable.selectAll")} selectRowLabel={() => t("common.dataTable.selectRow")} selectedLabel={(count) => t("common.dataTable.selected", { count })} mobileBackLabel={t("common.actions.back")} createAction={<Button size="icon-sm" aria-label={t("users.createEntity", { entity: t("access.subjectKinds.user") })} onClick={onCreate}><PlusIcon /></Button>} batchActions={(rows, done) => <BatchActions entity="users" rows={rows} queryKeys={["users", "user-keys"]} onApplied={done} size="xs" />} emptyState={<Empty><EmptyHeader><EmptyMedia variant="icon"><UsersIcon /></EmptyMedia><EmptyTitle>{t("users.selectEntity", { entity: t("access.subjectKinds.user") })}</EmptyTitle></EmptyHeader></Empty>}>
      {selected ? <UserDetail user={selected} organizations={access.organizations} teams={access.teams} access={access} panel={panel} pending={pending} onPanel={onPanel} onSave={(value, password) => onSave(selected, value, password)} onDeleted={onBack} /> : null}
    </WorkspaceLayout>
  )
}
