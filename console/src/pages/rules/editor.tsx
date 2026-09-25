import { useState } from "react"
import { useMutation, useQueries, useQuery, useQueryClient } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import { applyPreset, bindings, ensureProviderDefaultSet, providerDefaultSetId, rulePresets, rules } from "@/api/routing-rules"
import { directory } from "@/api/models"
import type { ProviderRuleSetDto, RewriteRuleDto, RewriteRuleWrite, RuleSetDto } from "@/generated/sdk"
import { DataTable } from "@/components/data-table"
import { BoolCell } from "@/components/cells"
import { ConfirmButton } from "@/components/confirm"
import { EmptyNotice, ErrorNotice, QueryState } from "@/components/state"
import { Button } from "@/components/ui/button"
import { Badge } from "@/components/ui/badge"
import { Dialog, DialogBody, DialogContent, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { FieldDescription, FieldGroup } from "@/components/ui/field"
import { RuleForm, RuleSelect, type SetChoice } from "./rule-form"

import { ruleKind } from "./rule-kind"

type Props = { sets: RuleSetDto[]; availableSets?: RuleSetDto[]; attachments?: ProviderRuleSetDto[]; providerId?: string; providerName?: string; defaultSetId: string }
const ordered = (a: RewriteRuleDto, b: RewriteRuleDto) => a.sortOrder - b.sortOrder || (a.id < b.id ? -1 : a.id > b.id ? 1 : 0)
export function RulesEditor({ sets, availableSets = sets, attachments = [], providerId, providerName, defaultSetId }: Props) {
  const { t } = useTranslation()
  const client = useQueryClient()
  const sharedBindings = useQuery({ queryKey: ["admin", "/provider-rule-sets", "directory"], queryFn: () => directory(bindings), enabled: !providerId })
  const allAttachments = providerId ? attachments : sharedBindings.data ?? []
  const [editing, setEditing] = useState<RewriteRuleDto | "new" | null>(null)
  const [showPresets, setShowPresets] = useState(false)
  const lists = useQueries({ queries: sets.map(set => ({ queryKey: ["admin", "/rules", set.id, "editor"], queryFn: () => directory(rules, { ruleSetId: set.id }) })) })
  const rows = lists.flatMap(list => [...(list.data ?? [])].sort(ordered))
  const choices: SetChoice[] = availableSets.map(set => ({ id: set.id, name: set.id === defaultSetId ? t("rules.defaultSet", { name: set.name }) : set.name, shared: new Set([...allAttachments.filter(binding => binding.ruleSetId === set.id).map(binding => binding.providerId), ...(providerId ? [providerId] : [])]).size > 1 }))
  if (!choices.some(set => set.id === defaultSetId)) choices.unshift({ id: defaultSetId, name: t("rules.defaultSet", { name: providerName }) })
  const invalidate = () => Promise.all(["/rules", "/rule-sets", "/provider-rule-sets"].map(path => client.invalidateQueries({ queryKey: ["admin", path] })))
  const ensureTarget = async (setId: string) => {
    if (!providerId || attachments.some(binding => binding.providerId === providerId && binding.ruleSetId === setId)) return
    if (setId === providerDefaultSetId(providerId)) await ensureProviderDefaultSet(providerId)
    else {
      const attached = attachments.filter(binding => binding.providerId === providerId)
      await bindings.create({ providerId, ruleSetId: setId, sortOrder: Math.max(-1, ...attached.map(binding => binding.sortOrder)) + 1, enabled: true })
    }
    await invalidate()
  }
  const save = useMutation({
    mutationFn: async (write: RewriteRuleWrite) => {
      if (editing && editing !== "new") {
        const patch: Partial<RewriteRuleWrite> = { ...write }
        delete patch.id
        delete patch.ruleSetId
        return rules.update(editing.id, patch)
      }
      const setId = write.ruleSetId!
      await ensureTarget(setId)
      const existing = await directory(rules, { ruleSetId: setId })
      return rules.create({ ...write, sortOrder: Math.max(-1, ...existing.map(row => row.sortOrder)) + 1 })
    },
    onSuccess: async () => { setEditing(null); await invalidate(); toast.success(t("toast.saved")) },
  })
  const remove = useMutation({ mutationFn: (id: string) => rules.remove(id), onSuccess: async () => { await invalidate(); toast.success(t("toast.deleted")) } })
  const move = useMutation({ mutationFn: ({ row, delta }: { row: RewriteRuleDto; delta: number }) => {
    const group = rows.filter(rule => rule.ruleSetId === row.ruleSetId)
    const index = group.findIndex(rule => rule.id === row.id)
    ;[group[index], group[index + delta]] = [group[index + delta], group[index]]
    return rules.batch(group.map((rule, sortOrder) => ({ update: { id: rule.id, patch: { sortOrder } } })))
  }, onSuccess: async () => { await invalidate(); toast.success(t("toast.saved")) } })
  const apply = useMutation({ mutationFn: async ({ setId, preset }: { setId: string; preset: string }) => { await ensureTarget(setId); return applyPreset(setId, preset) }, onSuccess: async () => { setShowPresets(false); await invalidate(); toast.success(t("toast.saved")) } })
  const busy = save.isPending || remove.isPending || move.isPending || apply.isPending
  const active = (row: RewriteRuleDto) => row.enabled && sets.find(set => set.id === row.ruleSetId)?.enabled && (!providerId || attachments.some(binding => binding.providerId === providerId && binding.ruleSetId === row.ruleSetId && binding.enabled))
  const summary = (row: RewriteRuleDto) => {
    if (row.action === "system_text") return JSON.parse(row.replacement).text as string
    if (row.action === "cache_breakpoint") { const config = JSON.parse(row.replacement); return `${t(`rules.protocols.${config.dialect}`)} · ${t(`rules.options.${config.target}`)}` }
    return [row.targetName || row.paths?.join(", "), row.action === "replace" ? row.pattern : t(`rules.options.${row.action}`), row.replacement].filter(Boolean).join(" · ")
  }
  return <>
    <div className="mb-4 flex flex-wrap gap-2"><Button disabled={busy} onClick={() => { save.reset(); setEditing("new") }}>{t("create.rules")}</Button><Button variant="outline" disabled={busy} onClick={() => { apply.reset(); setShowPresets(true) }}>{t("rules.applyPreset")}</Button></div>
    {remove.error || move.error ? <ErrorNotice error={remove.error ?? move.error} /> : null}
    <QueryState isPending={lists.some(list => list.isPending)} error={lists.find(list => list.error)?.error}>
      <DataTable rows={rows} rowKey={row => row.id} empty={<EmptyNotice title={t("rules.empty")} />} columns={[
        { key: "type", header: t("rules.ruleType"), cell: row => t(`rules.types.${ruleKind(row)}`) },
        { key: "rule", header: t("rules.content"), cell: row => <span className="block max-w-lg truncate" title={summary(row)}>{summary(row)}</span> },
        { key: "ruleSetId", cell: row => <span className="inline-flex flex-wrap items-center gap-2">{choices.find(set => set.id === row.ruleSetId)?.name}{choices.find(set => set.id === row.ruleSetId)?.shared ? <Badge variant="outline">{t("rules.shared")}</Badge> : null}</span> },
        { key: "phase", cell: row => t(`rules.options.${row.phase}`) },
        { key: "enabled", cell: row => <span className="inline-flex items-center gap-2"><BoolCell value={row.enabled} />{row.enabled && !active(row) ? <Badge variant="outline">{t("rules.inactive")}</Badge> : null}</span> },
      ]} actions={row => {
        const group = rows.filter(rule => rule.ruleSetId === row.ruleSetId)
        return <><Button variant="ghost" size="sm" disabled={busy || group[0].id === row.id} onClick={() => move.mutate({ row, delta: -1 })}>{t("management.moveUp")}</Button><Button variant="ghost" size="sm" disabled={busy || group.at(-1)?.id === row.id} onClick={() => move.mutate({ row, delta: 1 })}>{t("management.moveDown")}</Button><Button variant="ghost" size="sm" disabled={busy} onClick={() => { save.reset(); setEditing(row) }}>{t("actions.edit")}</Button><ConfirmButton disabled={busy} title={t("confirm.deleteTitle", { name: summary(row) })} onConfirm={() => remove.mutate(row.id)}>{t("actions.delete")}</ConfirmButton></>
      }} />
    </QueryState>
    {editing !== null ? <RuleForm key={editing === "new" ? "new" : editing.id} original={editing === "new" ? undefined : editing} choices={choices} defaultSetId={defaultSetId} pending={save.isPending} error={save.error} onClose={() => setEditing(null)} onSubmit={write => save.mutate(write)} /> : null}
    {showPresets ? <PresetDialog choices={choices} defaultSetId={defaultSetId} pending={apply.isPending} error={apply.error} onClose={() => setShowPresets(false)} onSubmit={(setId, preset) => apply.mutate({ setId, preset })} /> : null}
  </>
}

function PresetDialog({ choices, defaultSetId, pending, error, onClose, onSubmit }: { choices: SetChoice[]; defaultSetId: string; pending: boolean; error: unknown; onClose: () => void; onSubmit: (setId: string, preset: string) => void }) {
  const { t } = useTranslation()
  const [setId, setSetId] = useState(defaultSetId)
  const [preset, setPreset] = useState("")
  const presets = useQuery({ queryKey: ["rule-presets"], queryFn: rulePresets })
  return <Dialog open onOpenChange={open => { if (!open && !pending) onClose() }}><DialogContent closeLabel={t("actions.close")} aria-describedby={undefined}>
    <DialogHeader><DialogTitle>{t("rules.applyPreset")}</DialogTitle></DialogHeader>
    <DialogBody><QueryState isPending={presets.isPending} error={presets.error}><FieldGroup>
      <RuleSelect label={t("fields.ruleSetId")} value={setId} options={choices.map(set => ({ value: set.id, label: set.name }))} onChange={setSetId} disabled={pending} />
      {choices.find(set => set.id === setId)?.shared ? <FieldDescription>{t("rules.sharedWarning")}</FieldDescription> : null}
      <RuleSelect label={t("rules.preset")} value={preset} options={(presets.data ?? []).map(preset => ({ value: preset.id, label: preset.name }))} onChange={setPreset} disabled={pending} />
      <FieldDescription>{t("rules.presetConfirm")}</FieldDescription>
    </FieldGroup></QueryState>{error ? <ErrorNotice error={error} /> : null}</DialogBody>
    <DialogFooter><Button disabled={pending || !preset} onClick={() => onSubmit(setId, preset)}>{t("rules.apply")}</Button></DialogFooter>
  </DialogContent></Dialog>
}
