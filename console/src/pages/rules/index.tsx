import { useState } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { ArrowLeft, Plus } from "lucide-react"
import { toast } from "sonner"
import { bindings, ruleSetDirectory, ruleSets } from "@/api/routing-rules"
import { directory } from "@/api/models"
import { invalidateConfiguration } from "@/api/invalidation"
import type { RuleSetDto } from "@/generated/sdk"
import { useConfigBatch } from "@/components/config-batch"
import { ConfirmButton } from "@/components/confirm"
import { RecordDialog, type FormField } from "@/components/record-form"
import { ResizableColumns } from "@/components/resizable-columns"
import { EmptyNotice, ErrorNotice, QueryState } from "@/components/state"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { cn } from "@/lib/utils"
import { RulesEditor } from "./editor"

const fields: FormField[] = [{ name: "name", kind: "text", required: true }, { name: "description", kind: "text", nullable: true }, { name: "enabled", kind: "switch" }]

export function RuleSetsPage() {
  const { t } = useTranslation(), client = useQueryClient()
  const [selectedId, setSelectedId] = useState<string | null>(null)
  const [search, setSearch] = useState("")
  const [editing, setEditing] = useState<RuleSetDto | "new" | null>(null)
  const sets = useQuery({ queryKey: ["admin", "/rule-sets", "directory"], queryFn: ruleSetDirectory })
  const attached = useQuery({ queryKey: ["admin", "/provider-rule-sets", "directory"], queryFn: () => directory(bindings) })
  const selected = sets.data?.find(set => set.id === selectedId)
  const rows = (sets.data ?? []).filter(set => `${set.name} ${set.description ?? ""}`.toLocaleLowerCase().includes(search.trim().toLocaleLowerCase()))
  const invalidate = () => invalidateConfiguration(client, "/rule-sets")
  const save = useMutation({
    mutationFn: (body: Record<string, unknown>) => editing && editing !== "new" ? ruleSets.update(editing.id, body) : ruleSets.create(body),
    onSuccess: async row => { setEditing(null); setSelectedId(row.id); await invalidate(); toast.success(t("toast.saved")) },
  })
  const remove = useMutation({ mutationFn: (id: string) => ruleSets.remove(id), onSuccess: async () => { setSelectedId(null); await invalidate(); toast.success(t("toast.deleted")) } })
  const batch = useConfigBatch({ family: ruleSets, context: search, rows })
  const countProviders = (id: string) => new Set((attached.data ?? []).filter(binding => binding.ruleSetId === id).map(binding => binding.providerId)).size
  return <>
    <ResizableColumns storageKey="gproxy.layout.rules.v1" initialWidth={280} minWidth={224} maxWidth={480} contentMinWidth={360} label={t("nav.rule-sets")} className="min-h-[calc(100dvh-9rem)] overflow-hidden rounded-xl border bg-background">
      <section aria-label={t("nav.rule-sets")} className={cn("min-w-0 flex-col lg:flex lg:max-h-[calc(100dvh-9rem)]", selected ? "hidden" : "flex")}>
        <div className="flex flex-col gap-3 border-b p-3">
          <div className="flex items-center justify-between gap-2"><h1 className="text-lg font-medium">{t("nav.rule-sets")}</h1><div className="flex items-center gap-2">{batch.trigger}<Button size="icon-sm" aria-label={t("create.rule-sets")} onClick={() => { save.reset(); setEditing("new") }}><Plus /></Button></div></div>
          <Input aria-label={t("actions.search")} placeholder={t("actions.search")} value={search} onChange={event => setSearch(event.target.value)} />
        </div>
        {batch.active ? <div className="p-3">{batch.toolbar}</div> : null}
        <div className="min-h-0 flex-1 overflow-y-auto p-2">
          <QueryState isPending={sets.isPending} error={sets.error}>
            {rows.length ? <ul className="flex flex-col gap-2">{rows.map(set => <li key={set.id} className="flex items-center gap-1">
              {batch.checkbox(set.id, set.name)}
              <button type="button" aria-current={selected?.id === set.id ? "true" : undefined} onClick={() => setSelectedId(set.id)} className={cn("flex min-w-0 flex-1 flex-col gap-2 rounded-lg border p-3 text-left text-sm outline-none focus-visible:ring-2 focus-visible:ring-ring", selected?.id === set.id ? "border-primary bg-accent text-accent-foreground" : "hover:bg-muted")}>
                <span className="break-words font-medium">{set.name}</span>
                {set.description ? <span className="line-clamp-2 text-xs text-muted-foreground">{set.description}</span> : null}
                <span className="flex flex-wrap gap-2"><Badge variant={set.enabled ? "secondary" : "outline"}>{t(set.enabled ? "fields.enabled" : "values.disabled")}</Badge>{attached.data ? <Badge variant="outline">{t("rules.providerCount", { count: countProviders(set.id) })}</Badge> : null}</span>
              </button>
            </li>)}</ul> : <EmptyNotice title={t("state.emptyTitle")} />}
          </QueryState>
        </div>
      </section>
      <section className={cn("min-w-0 p-4", selected ? "block" : "hidden lg:block")}>
        {selected ? <div className="flex flex-col gap-4">
          <Button variant="ghost" className="self-start lg:hidden" onClick={() => setSelectedId(null)}><ArrowLeft data-icon="inline-start" />{t("nav.rule-sets")}</Button>
          <header className="flex flex-wrap items-start justify-between gap-3 border-b pb-4">
            <div className="min-w-0"><h2 className="break-words text-lg font-medium">{t("rules.title", { name: selected.name })}</h2>{selected.description ? <p className="mt-1 text-sm text-muted-foreground">{selected.description}</p> : null}</div>
            <div className="flex flex-wrap items-center gap-2"><Button variant="outline" size="sm" onClick={() => { save.reset(); setEditing(selected) }}>{t("actions.edit")}</Button><ConfirmButton disabled={remove.isPending} title={t("confirm.deleteTitle", { name: selected.name })} onConfirm={() => remove.mutate(selected.id)}>{t("actions.delete")}</ConfirmButton></div>
          </header>
          {remove.error ? <ErrorNotice error={remove.error} /> : null}
          {countProviders(selected.id) > 1 ? <p className="text-sm text-muted-foreground">{t("rules.sharedWarning")}</p> : null}
          <RulesEditor key={selected.id} sets={[selected]} defaultSetId={selected.id} />
        </div> : <EmptyNotice title={t("rules.selectSet")} />}
      </section>
    </ResizableColumns>
    <RecordDialog mode={editing === "new" ? "create" : "edit"} open={editing !== null} onOpenChange={open => { if (!open && !save.isPending) setEditing(null) }} title={t(editing === "new" ? "create.rule-sets" : "edit.rule-sets")} fields={fields} original={editing && editing !== "new" ? editing : undefined} pending={save.isPending} error={save.error} onSubmit={body => save.mutate(body)} />
  </>
}
