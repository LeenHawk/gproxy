import { useState } from "react"
import { useIsMutating } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { ruleSets } from "@/api/routing-rules"
import type { RuleSetDto } from "@/generated/sdk"
import { CollectionPage } from "@/pages/identity/collection"
import { BoolCell } from "@/components/cells"
import { Button } from "@/components/ui/button"
import { Dialog, DialogBody, DialogContent, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { RulesEditor } from "./editor"

export function RuleSetsPage() {
  const { t } = useTranslation()
  const [selected, setSelected] = useState<RuleSetDto | null>(null)
  return <>
    <CollectionPage id="rule-sets" family={ruleSets} searchable rowId={(r) => r.id} rowLabel={(r) => r.name}
      columns={[{ key: "name", cell: (r) => r.name }, { key: "description", cell: (r) => r.description ?? "—" }, { key: "enabled", cell: (r) => <BoolCell value={r.enabled} /> }]}
      fields={[{ name: "name", kind: "text", required: true }, { name: "description", kind: "text", nullable: true }, { name: "enabled", kind: "switch" }]}
      rowActions={(r) => <Button variant="ghost" size="sm" onClick={() => setSelected(r)}>{t("rules.manage")}</Button>}
    />
    {selected ? <RuleDetails key={selected.id} set={selected} onClose={() => setSelected(null)} /> : null}
  </>
}
function RuleDetails({ set, onClose }: { set: RuleSetDto; onClose: () => void }) {
  const { t } = useTranslation()
  const busy = useIsMutating() > 0
  return <Dialog open onOpenChange={open => { if (!open && !busy) onClose() }}><DialogContent className="sm:max-w-6xl" closeLabel={t("actions.close")} aria-describedby={undefined}>
    <DialogHeader><DialogTitle>{t("rules.title", { name: set.name })}</DialogTitle></DialogHeader>
    <DialogBody><RulesEditor sets={[set]} defaultSetId={set.id} /></DialogBody>
  </DialogContent></Dialog>
}
