import { useState } from "react"
import { useIsMutating, useMutation, useQueryClient } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import { apiKeys } from "@/api/admin"
import { ManagementDialog } from "@/components/management-dialog"
import { RecordEditor, type FormField } from "@/components/record-form"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { QuotasPanel } from "@/pages/quotas"
import type { ApiKeyDto, PortalKeyDto } from "@/generated/app"

export function KeySettingsDialog({ apiKey, fields, initialTab = "basic", onClose }: { apiKey: ApiKeyDto | PortalKeyDto; fields: readonly FormField[]; initialTab?: "basic" | "budget"; onClose: () => void }) {
  const { t } = useTranslation()
  const client = useQueryClient()
  const [tab, setTab] = useState(initialTab)
  const [current, setCurrent] = useState(apiKey)
  const busy = useIsMutating() > 0
  const save = useMutation({ mutationFn: (body: Record<string, unknown>) => apiKeys.update(apiKey.id, body as Parameters<typeof apiKeys.update>[1]), onSuccess: async row => {
    setCurrent(row); await Promise.all([client.invalidateQueries({ queryKey: ["admin", "/api-keys"] }), client.invalidateQueries({ queryKey: ["portal", "keys"] })]); toast.success(t("toast.saved"))
  } })
  return <ManagementDialog title={current.name} className="sm:max-w-2xl" busy={busy} onClose={onClose}>
    <Tabs value={tab} onValueChange={value => setTab(value as typeof tab)}><TabsList variant="line"><TabsTrigger value="basic" disabled={busy}>{t("limits.basic")}</TabsTrigger><TabsTrigger value="budget" disabled={busy}>{t("limits.budget")}</TabsTrigger></TabsList>
      <TabsContent value="basic" forceMount hidden={tab !== "basic"}><RecordEditor key={JSON.stringify(current)} fields={fields} original={{ ...current }} mode="edit" onSubmit={body => save.mutate(body)} pending={save.isPending} error={save.error} /></TabsContent>
      <TabsContent value="budget">{tab === "budget" ? <QuotasPanel ownerKind="api_key" ownerId={apiKey.id} /> : null}</TabsContent>
    </Tabs>
  </ManagementDialog>
}
