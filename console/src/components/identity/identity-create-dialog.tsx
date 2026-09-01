import { useTranslation } from "react-i18next"
import type { OrganizationDto } from "@/generated/OrganizationDto"
import type { TeamDto } from "@/generated/TeamDto"
import { IdentityForm, type IdentityKind } from "@/components/keys/identity-forms"
import { Dialog, DialogBody, DialogContent, DialogHeader, DialogTitle } from "@/components/ui/dialog"

type Props = {
  open: boolean
  onOpenChange: (open: boolean) => void
  kind: IdentityKind
  organizations: Array<OrganizationDto>
  teams: Array<TeamDto>
  pending: boolean
  onOrganization: (name: string) => Promise<void>
  onTeam: (organizationId: number, name: string) => Promise<void>
  onUser: (organizationId: number | null, teamId: number | null, name: string, password: string) => Promise<void>
}

export function IdentityCreateDialog(props: Props) {
  const { t } = useTranslation()
  const label = t(`users.entities.${props.kind}`)
  return (
    <Dialog open={props.open} onOpenChange={props.onOpenChange}>
      <DialogContent>
        <DialogHeader><DialogTitle>{t("users.createEntity", { entity: label })}</DialogTitle></DialogHeader>
        <DialogBody>
          <IdentityForm {...props} />
        </DialogBody>
      </DialogContent>
    </Dialog>
  )
}
