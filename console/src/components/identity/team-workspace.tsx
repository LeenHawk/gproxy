import { useMemo } from "react"
import { PlusIcon, UsersRoundIcon } from "lucide-react"
import { useTranslation } from "react-i18next"
import type { TeamDto } from "@/generated/TeamDto"
import type { AccessManagerProps } from "@/components/keys/access-manager"
import { BatchActions } from "@/components/batch-actions"
import { TeamDetail, type TeamPanel } from "@/components/identity/team-detail"
import { Button } from "@/components/ui/button"
import { Empty, EmptyHeader, EmptyMedia, EmptyTitle } from "@/components/ui/empty"
import { Switch } from "@/components/ui/switch"
import { WorkspaceLayout } from "@/components/workspace/workspace-layout"

export function TeamWorkspace({ access, selectedId, panel, pending, onSelect, onBack, onPanel, onCreate, onToggle }: {
  access: AccessManagerProps
  selectedId: number | null
  panel: TeamPanel
  pending: boolean
  onSelect: (team: TeamDto) => void
  onBack: () => void
  onPanel: (panel: TeamPanel) => void
  onCreate: () => void
  onToggle: (team: TeamDto) => void
}) {
  const { t } = useTranslation()
  const organizationNames = useMemo(() => new Map(access.organizations.map((value) => [value.id, value.name])), [access.organizations])
  const selected = access.teams.find((team) => team.id === selectedId)
  const summary = (team: TeamDto) => organizationNames.get(team.organization_id) ?? `#${team.organization_id}`
  return (
    <WorkspaceLayout storageKey="identity-teams" title={t("users.fields.team")} items={access.teams} selectedId={selected?.id ?? null} getSearchText={(team) => `${team.name} ${summary(team)}`} renderTitle={(team) => team.name} renderSummary={summary} renderAction={(team) => <Switch checked={team.enabled} disabled={pending} aria-label={`${t(team.enabled ? "common.actions.disable" : "common.actions.enable")} ${team.name}`} onCheckedChange={() => onToggle(team)} />} onSelect={onSelect} onBack={onBack} searchPlaceholder={t("users.searchEntity", { entity: t("users.fields.team") })} emptyLabel={t("common.none")} resizeLabel={t("nav.resize")} selectAllLabel={t("common.dataTable.selectAll")} selectRowLabel={() => t("common.dataTable.selectRow")} selectedLabel={(count) => t("common.dataTable.selected", { count })} mobileBackLabel={t("common.actions.back")} createAction={<Button size="icon-sm" aria-label={t("users.createEntity", { entity: t("users.fields.team") })} onClick={onCreate}><PlusIcon /></Button>} batchActions={(rows, done) => <BatchActions entity="teams" rows={rows} queryKeys={["teams", "users"]} onApplied={done} size="xs" />} emptyState={<Empty><EmptyHeader><EmptyMedia variant="icon"><UsersRoundIcon /></EmptyMedia><EmptyTitle>{t("users.selectEntity", { entity: t("users.fields.team") })}</EmptyTitle></EmptyHeader></Empty>}>
      {selected ? <TeamDetail team={selected} organizations={access.organizations} access={access} panel={panel} onPanel={onPanel} onDeleted={onBack} /> : null}
    </WorkspaceLayout>
  )
}
