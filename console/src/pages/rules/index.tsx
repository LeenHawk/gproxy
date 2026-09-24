import { useState } from "react"
import { useIsMutating, useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import { ruleSets, rules, rulePresets, applyPreset } from "@/api/routing-rules"
import type { RuleSetDto, RewriteRuleWrite } from "@/generated/sdk"
import { api, json } from "@/api/client"
import { directory } from "@/api/models"
import { RecordDialog, type FormField } from "@/components/record-form"
import { DataTable } from "@/components/data-table"
import { CollectionPage } from "@/pages/identity/collection"
import { BoolCell } from "@/components/cells"
import { ConfirmButton } from "@/components/confirm"
import { EmptyNotice, ErrorNotice, QueryState } from "@/components/state"
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
  const [presetRevision, setPresetRevision] = useState(0)
  const busy = useIsMutating() > 0
  const presets = useQuery({ queryKey: ["rule-presets"], queryFn: rulePresets })
  const applied = useMutation({ mutationFn: () => applyPreset(set.id, preset), onSuccess: async () => { await client.invalidateQueries({ queryKey: ["admin", "/rules"] }); setPresetRevision(value => value + 1); toast.success(t("toast.saved")) } })
  return <Dialog open onOpenChange={(open) => { if (!open && !applied.isPending) onClose() }}><DialogContent className="sm:max-w-6xl" closeLabel={t("actions.close")} aria-describedby={undefined}>
    <DialogHeader><DialogTitle>{t("rules.title", { name: set.name })}</DialogTitle></DialogHeader>
    <DialogBody>
      <QueryState isPending={presets.isPending} error={presets.error}><div className="mb-4 flex flex-wrap items-center gap-2">
        <Select value={preset} onValueChange={setPreset}><SelectTrigger aria-label={t("rules.preset")}><SelectValue placeholder={t("rules.preset")} /></SelectTrigger><SelectContent><SelectGroup>{presets.data?.map((p) => <SelectItem key={p.id} value={p.id}>{p.name}</SelectItem>)}</SelectGroup></SelectContent></Select>
        <ConfirmButton disabled={!preset || busy} title={t("rules.presetConfirm")} confirmLabel={t("rules.apply")} onConfirm={() => applied.mutate()}>{t("rules.apply")}</ConfirmButton>
      </div></QueryState>
      {applied.error ? <ErrorNotice error={applied.error} /> : null}
      <RuleEditor key={presetRevision} setId={set.id} locked={applied.isPending} />
    </DialogBody>
  </DialogContent></Dialog>
}


function RuleEditor({ setId, locked }: { setId: string; locked: boolean }) {
  const { t } = useTranslation()
  const client = useQueryClient()
  const list = useQuery({ queryKey: ["admin", "/rules", setId, "editor"], queryFn: () => directory(rules, { ruleSetId: setId }) })
  const [draft, setDraft] = useState<RewriteRuleWrite[] | null>(null)
  const [editing, setEditing] = useState<number | "new" | null>(null)
  const rows: RewriteRuleWrite[] = draft ?? [...(list.data ?? [])].sort((a, b) => a.sortOrder - b.sortOrder)
  const fields: FormField[] = [{ name: "action", kind: "select", options: ["replace", "set"] }, { name: "phase", kind: "select", options: ["request", "response", "both"] }, { name: "target", kind: "select", options: ["body", "header", "query"] }, { name: "targetName", kind: "text", nullable: true }, { name: "paths", kind: "lines", nullable: true }, { name: "pattern", kind: "text", required: true }, { name: "replacement", kind: "text", nullable: true }, { name: "filterModelPattern", kind: "text", nullable: true }, { name: "filterHeaderPattern", kind: "text", nullable: true }, { name: "filterEventPattern", kind: "text", nullable: true }, { name: "filterOperationKeys", kind: "json", nullable: true }, { name: "sortOrder", kind: "number" }, { name: "enabled", kind: "switch" }]
  const save = useMutation({ mutationFn: () => api(`/admin/api/rule-sets/${encodeURIComponent(setId)}/rules`, json("PUT", rows.map((row, index) => ({ ...row, ruleSetId: setId, sortOrder: index })))),
    onSuccess: async () => { await client.invalidateQueries({ queryKey: ["admin", "/rules"] }); setDraft(null); toast.success(t("toast.saved")) } })
  const move = (index: number, delta: number) => { const next = [...rows]; [next[index], next[index + delta]] = [next[index + delta], next[index]]; setDraft(next) }
  const original = typeof editing === "number" ? rows[editing] : undefined
  return <QueryState isPending={list.isPending} error={list.error}>
    <div className="mb-3 flex flex-wrap gap-2"><Button disabled={locked || save.isPending} onClick={() => setEditing("new")}>{t("actions.new")}</Button><Button disabled={locked || !draft || save.isPending} onClick={() => save.mutate()}>{t("management.saveRules")}</Button><Button variant="outline" disabled={locked || !draft || save.isPending} onClick={() => setDraft(null)}>{t("actions.cancel")}</Button></div>
    <DataTable rows={rows} rowKey={row => row.id!} empty={<EmptyNotice title={t("state.emptyTitle")} />} columns={[{ key: "pattern", cell: row => row.pattern }, { key: "replacement", cell: row => row.replacement }, { key: "phase", cell: row => row.phase }, { key: "enabled", cell: row => <BoolCell value={row.enabled ?? true} /> }]} actions={row => {
      const index = rows.indexOf(row)
      return <><Button size="sm" variant="ghost" disabled={locked || save.isPending || index === 0} onClick={() => move(index, -1)}>{t("management.moveUp")}</Button><Button size="sm" variant="ghost" disabled={locked || save.isPending || index === rows.length - 1} onClick={() => move(index, 1)}>{t("management.moveDown")}</Button><Button size="sm" variant="ghost" disabled={locked || save.isPending} onClick={() => setEditing(index)}>{t("actions.edit")}</Button><ConfirmButton disabled={locked || save.isPending} title={t("confirm.deleteTitle", { name: row.pattern })} onConfirm={() => setDraft(rows.filter((_, current) => current !== index))}>{t("actions.delete")}</ConfirmButton></>
    }} />
    {save.error ? <ErrorNotice error={save.error} /> : null}
    {editing !== null ? <RecordDialog open title={t(original ? "edit.rules" : "create.rules")} mode={original ? "edit" : "create"} original={original ? { ...original } : { phase: "request", action: "replace", target: "body", enabled: true }} fields={fields.filter(field => field.name !== "sortOrder")} onOpenChange={open => { if (!open) setEditing(null) }} onSubmit={body => {
      const next = [...rows]
      if (typeof editing === "number") next[editing] = { ...next[editing], ...body, replacement: "replacement" in body ? String(body.replacement ?? "") : next[editing].replacement }
      else next.push({ id: crypto.randomUUID(), ruleSetId: setId, phase: "request", action: "replace", target: "body", targetName: null, paths: null, pattern: "", replacement: "", filterOperationKeys: null, filterModelPattern: null, filterHeaderPattern: null, filterEventPattern: null, sortOrder: next.length, enabled: true, ...body } as RewriteRuleWrite)
      setDraft(next); setEditing(null)
    }} /> : null}
  </QueryState>
}
