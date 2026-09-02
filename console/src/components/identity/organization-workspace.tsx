import { Building2Icon, PlusIcon } from "lucide-react"
import { useTranslation } from "react-i18next"
import type { OrganizationDto } from "@/generated/OrganizationDto"
import type { OrganizationWriteRequest } from "@/generated/OrganizationWriteRequest"
import type { TeamDto } from "@/generated/TeamDto"
import type { AccessManagerProps } from "@/components/keys/access-manager"
import { BatchActions } from "@/components/batch-actions"
import { EntityDeleteButton } from "@/components/entity-delete-button"
import { OrganizationDetail, type OrganizationPanel } from "@/components/identity/organization-detail"
import { Button } from "@/components/ui/button"
import { Empty, EmptyHeader, EmptyMedia, EmptyTitle } from "@/components/ui/empty"
import { Switch } from "@/components/ui/switch"
import { WorkspaceLayout } from "@/components/workspace/workspace-layout"

export function OrganizationWorkspace({ access, selectedId, panel, pending, onSelect, onBack, onPanel, onTeam, onCreate, onToggle, onSave }: {
  access: AccessManagerProps
  selectedId: number | null
  panel: OrganizationPanel
  pending: boolean
  onSelect: (organization: OrganizationDto) => void
  onBack: () => void
  onPanel: (panel: OrganizationPanel) => void
  onTeam: (team: TeamDto) => void
  onCreate: () => void
  onToggle: (organization: OrganizationDto) => void
  onSave: (organization: OrganizationDto, value: OrganizationWriteRequest) => Promise<void>
}) {
  const { t } = useTranslation()
  const selected = access.organizations.find((organization) => organization.id === selectedId)
  const summary = (organization: OrganizationDto) => t("users.organizationSummary", { count: access.teams.filter((team) => team.organization_id === organization.id).length })
  return (
    <WorkspaceLayout storageKey="identity-organizations" title={t("users.fields.organization")} items={access.organizations} selectedId={selected?.id ?? null} getSearchText={(organization) => `${organization.name} ${summary(organization)}`} renderTitle={(organization) => organization.name} renderSummary={summary} renderAction={(organization) => <div className="flex items-center gap-1"><Switch checked={organization.enabled} disabled={pending} aria-label={`${t(organization.enabled ? "common.actions.disable" : "common.actions.enable")} ${organization.name}`} onCheckedChange={() => onToggle(organization)} /><EntityDeleteButton entity="organizations" id={organization.id} label={organization.name} queryKeys={["organizations", "teams", "users"]} onDeleted={organization.id === selected?.id ? onBack : undefined} /></div>} onSelect={onSelect} onBack={onBack} searchPlaceholder={t("users.searchEntity", { entity: t("users.fields.organization") })} emptyLabel={t("common.none")} resizeLabel={t("nav.resize")} selectAllLabel={t("common.dataTable.selectAll")} selectRowLabel={() => t("common.dataTable.selectRow")} selectedLabel={(count) => t("common.dataTable.selected", { count })} mobileBackLabel={t("common.actions.back")} createAction={<Button size="icon-sm" aria-label={t("users.createEntity", { entity: t("users.fields.organization") })} onClick={onCreate}><PlusIcon /></Button>} batchActions={(rows, done) => <BatchActions entity="organizations" rows={rows} queryKeys={["organizations", "teams", "users"]} onApplied={done} size="xs" />} emptyState={<Empty><EmptyHeader><EmptyMedia variant="icon"><Building2Icon /></EmptyMedia><EmptyTitle>{t("users.selectEntity", { entity: t("users.fields.organization") })}</EmptyTitle></EmptyHeader></Empty>}>
      {selected ? <OrganizationDetail organization={selected} teams={access.teams} access={access} panel={panel} pending={pending} onPanel={onPanel} onTeam={onTeam} onSave={(value) => onSave(selected, value)} onDeleted={onBack} /> : null}
    </WorkspaceLayout>
  )
}
