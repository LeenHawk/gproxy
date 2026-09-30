import { ChevronRight } from "lucide-react"
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible"
import { Pagination } from "@/components/data-table"
import { usePagination } from "@/lib/use-pagination"
import { useState, type ReactNode } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import { applyPreset, bindings, ensureProviderDefaultSet, providerDefaultSetId, rulePresets, rules } from "@/api/routing-rules"
import { directory } from "@/api/models"
import type { ProviderRuleSetDto, RewriteRuleDto, RewriteRuleWrite, RuleSetDto } from "@/generated/sdk"
import { ConfirmButton } from "@/components/confirm"
import { EmptyNotice, ErrorNotice, QueryState } from "@/components/state"
import { Button } from "@/components/ui/button"
import { Badge } from "@/components/ui/badge"
import { Card, CardContent, CardFooter, CardHeader, CardTitle } from "@/components/ui/card"
import { Dialog, DialogBody, DialogContent, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { FieldDescription, FieldGroup } from "@/components/ui/field"
import { RuleForm, RuleSelect, RuleSetSelect, type SetChoice } from "./rule-form"

import { ruleKind } from "./rule-kind"

type Props = { sets: RuleSetDto[]; availableSets?: RuleSetDto[]; attachments?: ProviderRuleSetDto[]; providerId?: string; providerName?: string; defaultSetId: string }
const ordered = (a: RewriteRuleDto, b: RewriteRuleDto) => a.sortOrder - b.sortOrder || (a.id < b.id ? -1 : a.id > b.id ? 1 : 0)
export function RulesEditor({ sets, availableSets = sets, attachments = [], providerId, providerName, defaultSetId }: Props) {
  const { t } = useTranslation()
  const client = useQueryClient()
  const [editing, setEditing] = useState<RewriteRuleDto | "new" | null>(null)
  const [showPresets, setShowPresets] = useState(false)
  const choices: SetChoice[] = availableSets.map(set => ({ id: set.id, name: providerId && set.id === defaultSetId ? t("rules.defaultSet", { name: set.name }) : set.name, shared: (set.providerCount ?? 0) + (providerId && !attachments.some(binding => binding.ruleSetId === set.id) ? 1 : 0) > 1 }))
  if (!choices.some(set => set.id === defaultSetId)) choices.unshift({ id: defaultSetId, name: t("rules.defaultSet", { name: providerName }) })
  const invalidate = () => Promise.all(["/rules", "/rule-sets", "/provider-rule-sets"].map(path => client.invalidateQueries({ queryKey: ["admin", path] })))
  const ensureTarget = async (setId: string) => {
    if (!providerId || attachments.some(binding => binding.providerId === providerId && binding.ruleSetId === setId)) return
    const existing = await bindings.list({ providerId, ruleSetId: setId, pageSize: 1 })
    if (existing.total) return
    if (setId === providerDefaultSetId(providerId)) await ensureProviderDefaultSet(providerId)
    else {
      const first = await bindings.list({ providerId, pageSize: 1 })
      const last = first.total > 1 ? await bindings.list({ providerId, pageSize: 1, page: first.total }) : first
      await bindings.create({ providerId, ruleSetId: setId, sortOrder: (last.items[0]?.sortOrder ?? -1) + 1, enabled: true })
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
      const first = await rules.list({ ruleSetId: setId, pageSize: 1 })
      const last = first.total > 1 ? await rules.list({ ruleSetId: setId, page: first.total, pageSize: 1 }) : first
      return rules.create({ ...write, sortOrder: (last.items[0]?.sortOrder ?? -1) + 1 })
    },
    onSuccess: async () => { setEditing(null); await invalidate(); toast.success(t("toast.saved")) },
  })
  const remove = useMutation({ mutationFn: (id: string) => rules.remove(id), onSuccess: async () => { await invalidate(); toast.success(t("toast.deleted")) } })
  const move = useMutation({ mutationFn: async ({ row, delta }: { row: RewriteRuleDto; delta: number }) => {
    const group = (await directory(rules, { ruleSetId: row.ruleSetId })).sort(ordered)
    const index = group.findIndex(rule => rule.id === row.id)
    if (index < 0 || !group[index + delta]) return
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
    <p className="mb-3 text-xs text-muted-foreground">{t("rules.orderHint")}</p>
    {sets.length ? <div className="flex flex-col gap-3">{sets.map(set => {
      const source = choices.find(choice => choice.id === set.id)
      return <RuleGroup key={set.id} set={set} source={source} defaultOpen={sets.length === 1} renderRule={(row, index, total) => {
        return <li key={row.id}><Card size="sm">
          <CardHeader><CardTitle headingLevel={3}><span className="flex flex-wrap items-center gap-2"><Badge variant="secondary">{index + 1}</Badge>{t(`rules.types.${ruleKind(row)}`)}<Badge variant="outline">{t(`rules.options.${row.phase}`)}</Badge>{!row.enabled ? <Badge variant="outline">{t("values.disabled")}</Badge> : !active(row) ? <Badge variant="outline">{t("rules.inactive")}</Badge> : null}</span></CardTitle></CardHeader>
          <CardContent className="flex flex-col gap-2">
            <p className="whitespace-pre-wrap break-words text-sm" aria-label={t("rules.content")}>{summary(row)}</p>
            <div className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground"><span>{source?.name}</span>{source?.shared ? <Badge variant="outline">{t("rules.shared")}</Badge> : null}<span>{row.filterModelPattern || t("rules.allModels")}</span></div>
          </CardContent>
          <CardFooter className="flex flex-wrap justify-end gap-1"><Button variant="ghost" size="sm" disabled={busy || index === 0} onClick={() => move.mutate({ row, delta: -1 })}>{t("management.moveUp")}</Button><Button variant="ghost" size="sm" disabled={busy || index === total - 1} onClick={() => move.mutate({ row, delta: 1 })}>{t("management.moveDown")}</Button><Button variant="ghost" size="sm" disabled={busy} onClick={() => { save.reset(); setEditing(row) }}>{t("actions.edit")}</Button><ConfirmButton disabled={busy} title={t("confirm.deleteTitle", { name: summary(row) })} onConfirm={() => remove.mutate(row.id)}>{t("actions.delete")}</ConfirmButton></CardFooter>
        </Card></li>
      }} />
    })}</div> : <EmptyNotice title={t("rules.empty")} />}
    {editing !== null ? <RuleForm key={editing === "new" ? "new" : editing.id} original={editing === "new" ? undefined : editing} choices={choices} remoteSets={!!providerId} defaultSetId={defaultSetId} pending={save.isPending} error={save.error} onClose={() => setEditing(null)} onSubmit={write => save.mutate(write)} /> : null}
    {showPresets ? <PresetDialog choices={choices} remoteSets={!!providerId} defaultSetId={defaultSetId} pending={apply.isPending} error={apply.error} onClose={() => setShowPresets(false)} onSubmit={(setId, preset) => apply.mutate({ setId, preset })} /> : null}
  </>
}

function RuleGroup({ set, source, defaultOpen, renderRule }: { set: RuleSetDto; source?: SetChoice; defaultOpen: boolean; renderRule: (row: RewriteRuleDto, index: number, total: number) => ReactNode }) {
  const { t } = useTranslation()
  const [open, setOpen] = useState(defaultOpen)
  const { page, pageSize, setPage, setPageSize } = usePagination("rewrite-rules", set.id)
  const request = { ruleSetId: set.id, page, pageSize }
  const list = useQuery({ queryKey: ["admin", "/rules", "editor", request], queryFn: () => rules.list(request), enabled: open })
  const currentPage = Math.min(page, Math.max(1, Math.ceil((list.data?.total ?? 0) / pageSize)))
  if (list.data && page !== currentPage) setPage(currentPage)
  return <Collapsible open={open} onOpenChange={setOpen} className="rounded-lg border">
    <CollapsibleTrigger asChild><Button variant="ghost" className="group h-auto w-full justify-start whitespace-normal p-3"><ChevronRight data-icon="inline-start" className="transition-transform group-data-[state=open]:rotate-90" /><span className="min-w-0 flex-1 text-left break-words">{source?.name ?? set.name}</span>{list.data ? <Badge variant="secondary">{list.data.total}</Badge> : null}{!set.enabled ? <Badge variant="outline">{t("values.disabled")}</Badge> : null}</Button></CollapsibleTrigger>
    <CollapsibleContent className="p-3 pt-0"><QueryState isPending={list.isPending} error={list.error}>
      {list.data?.items.length ? <ol start={list.data.offset + 1} className="flex flex-col gap-3">{list.data.items.map((row, index) => renderRule(row, list.data!.offset + index, list.data!.total))}</ol> : <EmptyNotice title={t("rules.empty")} />}
      <div className="mt-3"><Pagination page={currentPage} pageSize={pageSize} total={list.data?.total ?? 0} onPage={setPage} onPageSize={setPageSize} /></div>
    </QueryState></CollapsibleContent>
  </Collapsible>
}

function PresetDialog({ choices, remoteSets, defaultSetId, pending, error, onClose, onSubmit }: { choices: SetChoice[]; remoteSets: boolean; defaultSetId: string; pending: boolean; error: unknown; onClose: () => void; onSubmit: (setId: string, preset: string) => void }) {
  const { t } = useTranslation()
  const [setId, setSetId] = useState(defaultSetId)
  const [preset, setPreset] = useState("")
  const presets = useQuery({ queryKey: ["rule-presets"], queryFn: rulePresets })
  return <Dialog open onOpenChange={open => { if (!open && !pending) onClose() }}><DialogContent closeLabel={t("actions.close")} aria-describedby={undefined}>
    <DialogHeader><DialogTitle>{t("rules.applyPreset")}</DialogTitle></DialogHeader>
    <DialogBody><QueryState isPending={presets.isPending} error={presets.error}><FieldGroup>
      <RuleSetSelect value={setId} choices={choices} remote={remoteSets} onChange={setSetId} disabled={pending} />
      <RuleSelect label={t("rules.preset")} value={preset} options={(presets.data ?? []).map(preset => ({ value: preset.id, label: preset.name }))} onChange={setPreset} disabled={pending} />
      <FieldDescription>{t("rules.presetConfirm")}</FieldDescription>
    </FieldGroup></QueryState>{error ? <ErrorNotice error={error} /> : null}</DialogBody>
    <DialogFooter><Button disabled={pending || !preset} onClick={() => onSubmit(setId, preset)}>{t("rules.apply")}</Button></DialogFooter>
  </DialogContent></Dialog>
}
