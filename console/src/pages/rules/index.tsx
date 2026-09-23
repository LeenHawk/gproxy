import { useState } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import { ruleSets, rules, rulePresets, applyPreset } from "@/api/routing-rules"
import type { RuleSetDto } from "@/generated/sdk"
import { CollectionPage } from "@/pages/identity/collection"
import { BoolCell } from "@/components/cells"
import { ConfirmButton } from "@/components/confirm"
import { ErrorNotice, QueryState } from "@/components/state"
import { Button } from "@/components/ui/button"
import { Dialog, DialogBody, DialogContent, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"

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
  const client = useQueryClient()
  const [preset, setPreset] = useState("")
  const presets = useQuery({ queryKey: ["rule-presets"], queryFn: rulePresets })
  const applied = useMutation({ mutationFn: () => applyPreset(set.id, preset), onSuccess: async () => { await client.invalidateQueries({ queryKey: ["admin", "/rules"] }); toast.success(t("toast.saved")) } })
  return <Dialog open onOpenChange={(open) => { if (!open && !applied.isPending) onClose() }}><DialogContent className="sm:max-w-6xl" closeLabel={t("actions.close")} aria-describedby={undefined}>
    <DialogHeader><DialogTitle>{t("rules.title", { name: set.name })}</DialogTitle></DialogHeader>
    <DialogBody>
      <QueryState isPending={presets.isPending} error={presets.error}><div className="mb-4 flex flex-wrap items-center gap-2">
        <Select value={preset} onValueChange={setPreset}><SelectTrigger aria-label={t("rules.preset")}><SelectValue placeholder={t("rules.preset")} /></SelectTrigger><SelectContent><SelectGroup>{presets.data?.map((p) => <SelectItem key={p.id} value={p.id}>{p.name}</SelectItem>)}</SelectGroup></SelectContent></Select>
        <ConfirmButton disabled={!preset || applied.isPending} title={t("rules.presetConfirm")} confirmLabel={t("rules.apply")} onConfirm={() => applied.mutate()}>{t("rules.apply")}</ConfirmButton>
      </div></QueryState>
      {applied.error ? <ErrorNotice error={applied.error} /> : null}
      <CollectionPage embedded id="rules" family={rules} filter={{ ruleSetId: set.id }} create={(body) => rules.create({ replacement: "", ...body, ruleSetId: set.id })} rowId={(r) => r.id} rowLabel={(r) => r.pattern}
        columns={[{ key: "sortOrder", cell: (r) => r.sortOrder }, { key: "phase", cell: (r) => t(`values.${r.phase}`) }, { key: "target", cell: (r) => t(`values.${r.target}`) }, { key: "pattern", cell: (r) => r.pattern }, { key: "replacement", cell: (r) => r.replacement }, { key: "filterModelPattern", cell: (r) => r.filterModelPattern ?? "*" }, { key: "enabled", cell: (r) => <BoolCell value={r.enabled} /> }]}
        fields={[{ name: "phase", kind: "select", options: ["request", "response", "both"] }, { name: "target", kind: "select", options: ["body", "header", "query"] }, { name: "targetName", kind: "text", nullable: true }, { name: "paths", kind: "lines", nullable: true }, { name: "pattern", kind: "text", required: true }, { name: "replacement", kind: "text" }, { name: "filterModelPattern", kind: "text", nullable: true }, { name: "filterHeaderPattern", kind: "text", nullable: true }, { name: "filterEventPattern", kind: "text", nullable: true }, { name: "filterOperationKeys", kind: "json", nullable: true }, { name: "sortOrder", kind: "number" }, { name: "enabled", kind: "switch" }]}
      />
    </DialogBody>
  </DialogContent></Dialog>
}
