import { useState } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import { models, openrouterModels, directory } from "@/api/models"
import { matchingModel } from "@/lib/model-catalog"
import { object } from "@/components/providers/provider-model-state"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import { Input } from "@/components/ui/input"
import { Dialog, DialogBody, DialogContent, DialogFooter, DialogHeader, DialogTitle, DialogDescription } from "@/components/ui/dialog"
import { ErrorNotice, QueryState } from "@/components/state"
import { Pagination } from "@/components/data-table"
import { usePagination } from "@/lib/use-pagination"

export function ModelMetadataImport({ onClose }: { onClose: () => void }) {
  const { t } = useTranslation(), client = useQueryClient()
  const stored = useQuery({ queryKey: ["admin", "/models", "import"], queryFn: () => directory(models) })
  const existing = stored.data ?? []
  const remote = useQuery({ queryKey: ["openrouter-models"], queryFn: openrouterModels, retry: false })
  const [search, setSearch] = useState("")
  const [selected, setSelected] = useState<Set<string>>(new Set())
  const { page, pageSize, setPage, setPageSize } = usePagination("metadata-import", search)
  const rows = (remote.data ?? []).filter(row => `${row.upstreamName} ${object(row.metadata).display_name ?? ""}`.toLowerCase().includes(search.toLowerCase()))
  const all = rows.length > 0 && rows.every(row => selected.has(row.upstreamName))
  const save = useMutation({ mutationFn: async () => {
    const writes = (remote.data ?? []).filter(row => selected.has(row.upstreamName)).map(row => {
      const local = matchingModel(existing, row.upstreamName, row => row.name)
      const metadata = { ...object(row.metadata) }
      for (const [key, value] of Object.entries(object(local?.metadata))) if (value != null) metadata[key] = value
      return local ? { update: { id: local.id, patch: { metadata } } } : { create: { name: row.upstreamName, metadata } }
    })
    await models.batch(writes)
    return writes.length
  }, onSuccess: async count => {
    await Promise.all([client.invalidateQueries({ queryKey: ["model-catalog"] }), client.invalidateQueries({ queryKey: ["admin", "/models"] }), client.invalidateQueries({ queryKey: ["discover-models"] })])
    toast.success(t("catalog.metadataImported", { count })); onClose()
  } })
  return <Dialog open onOpenChange={open => { if (!open && !save.isPending) onClose() }}><DialogContent className="sm:max-w-2xl">
    <DialogHeader><DialogTitle>{t("catalog.importRemote")}</DialogTitle><DialogDescription>{t("catalog.importRemoteHelp")}</DialogDescription></DialogHeader>
    <DialogBody className="flex flex-col gap-3"><QueryState isPending={stored.isPending || remote.isPending} error={stored.error ?? remote.error}>
      <div className="flex gap-2"><Input aria-label={t("actions.search")} placeholder={t("actions.search")} value={search} onChange={e => setSearch(e.target.value)} /><Button variant="outline" disabled={remote.isFetching || save.isPending} onClick={() => void remote.refetch()}>{t("modelUI.refresh")}</Button></div>
      <label className="flex items-center gap-2"><Checkbox checked={all} onCheckedChange={() => setSelected(previous => { const next = new Set(previous); rows.forEach(row => all ? next.delete(row.upstreamName) : next.add(row.upstreamName)); return next })} />{t("modelUI.selectAll")} ({selected.size}/{remote.data?.length ?? 0})</label>
      <div className="flex flex-col divide-y">{rows.slice((page - 1) * pageSize, page * pageSize).map(row => <label key={row.upstreamName} className="flex items-center gap-3 py-3"><Checkbox checked={selected.has(row.upstreamName)} onCheckedChange={checked => setSelected(previous => { const next = new Set(previous); if (checked) next.add(row.upstreamName); else next.delete(row.upstreamName); return next })} /><span className="min-w-0 break-all">{row.upstreamName}</span></label>)}</div>
      <Pagination page={page} pageSize={pageSize} total={rows.length} onPage={setPage} onPageSize={setPageSize} />
    </QueryState>{save.error ? <ErrorNotice error={save.error} /> : null}</DialogBody>
    <DialogFooter><Button variant="outline" disabled={save.isPending} onClick={onClose}>{t("actions.cancel")}</Button><Button disabled={save.isPending || !selected.size || !stored.data} onClick={() => save.mutate()}>{t("modelUI.importSelected")}</Button></DialogFooter>
  </DialogContent></Dialog>
}
