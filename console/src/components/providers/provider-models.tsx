import { useState } from "react"
import { useMutation, useQueryClient } from "@tanstack/react-query"
import { BadgeDollarSignIcon, PencilIcon } from "lucide-react"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import { saveProviderModel } from "@/api/control"
import type { PriceRuleDto } from "@/generated/PriceRuleDto"
import type { ProviderModelDto } from "@/generated/ProviderModelDto"
import type { ProviderModelWriteRequest } from "@/generated/ProviderModelWriteRequest"
import { EntityDeleteButton } from "@/components/entity-delete-button"
import { ProviderModelDialog } from "@/components/providers/provider-model-dialog"
import { DataTable, type DataTableColumn } from "@/components/data-table"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Section } from "@/components/section"
import { navigateAdminPath } from "@/lib/admin-route"
import { Switch } from "@/components/ui/switch"

export function ProviderModels({ providerId, models, priceRules }: { providerId: number; models: Array<ProviderModelDto>; priceRules: Array<PriceRuleDto> }) {
  const { t, i18n } = useTranslation()
  const client = useQueryClient()
  const [editing, setEditing] = useState<ProviderModelDto>()
  const [open, setOpen] = useState(false)
  const mutation = useMutation({
    mutationFn: ({ value, id }: { value: ProviderModelWriteRequest; id?: number }) => saveProviderModel(value, id),
    onSuccess: async () => {
      await client.invalidateQueries({ queryKey: ["provider-models"] })
      toast.success(t("providers.models.saved"))
    },
    onError: () => toast.error(t("providers.models.saveError")),
  })
  const rows = models.filter((model) => model.provider_id === providerId)
  const openEditor = (model?: ProviderModelDto) => { setEditing(model); setOpen(true) }
  const number = (value: number | null) => value == null ? "—" : value.toLocaleString(i18n.language)
  const priceFor = (model: ProviderModelDto) => priceRules.find((rule) => rule.provider_id === providerId && rule.model_pattern === model.model_id)
  const variantCount = (model: ProviderModelDto) => {
    const value = model.variants
    if (Array.isArray(value)) return value.length
    if (value && typeof value === "object" && Array.isArray((value as { variants?: unknown }).variants)) return ((value as { variants: Array<unknown> }).variants).length
    return 0
  }

  const columns: Array<DataTableColumn<ProviderModelDto>> = [
    { key: "model", label: t("providers.models.modelId"), header: t("providers.models.modelId"), cell: (model) => <div><p className="machine-text text-xs">{model.model_id}</p>{model.display_name ? <p className="text-xs text-muted-foreground">{model.display_name}</p> : null}</div> },
    { key: "context", label: t("providers.models.contextWindow"), header: t("providers.models.contextWindow"), cell: (model) => <span className="machine-text text-xs">{number(model.context_window)}</span> },
    { key: "output", label: t("providers.models.maxOutput"), header: t("providers.models.maxOutput"), cell: (model) => <span className="machine-text text-xs">{number(model.max_output_tokens)}</span> },
    { key: "pricing", label: t("providers.models.pricing"), header: t("providers.models.pricing"), cell: (model) => priceFor(model) ? <Badge variant="secondary">{t("providers.models.priced")}</Badge> : <span className="text-xs text-muted-foreground">—</span> },
    { key: "variants", label: t("providers.models.variants"), header: t("providers.models.variants"), cell: (model) => variantCount(model) > 0 ? <Badge variant="outline">+{variantCount(model)}</Badge> : <span className="text-xs text-muted-foreground">—</span> },
    { key: "thinking", label: t("providers.models.thinking"), header: t("providers.models.thinking"), cell: (model) => model.thinking_supported == null ? <span className="text-xs text-muted-foreground">—</span> : <Badge variant={model.thinking_supported ? "outline" : "secondary"}>{t(model.thinking_supported ? "common.status.enabled" : "common.status.disabled")}</Badge> },
    { key: "enabled", label: t("providers.models.enabled"), header: t("providers.models.enabled"), cell: (model) => <Switch checked={model.enabled} disabled={mutation.isPending} aria-label={`${t("providers.models.enabled")}: ${model.model_id}`} onCheckedChange={(enabled) => mutation.mutate({ value: { ...request(model), enabled }, id: model.id })} /> },
    { key: "actions", label: t("common.actions.edit"), header: <span className="sr-only">{t("common.actions.edit")}</span>, className: "text-right", cell: (model) => (
      <div className="flex items-center justify-end gap-1">
        <Button size="icon-xs" variant="ghost" aria-label={`${t("providers.models.priceRule")}: ${model.model_id}`} onClick={() => navigateAdminPath(`/admin/pricing/${model.model_id}`)}><BadgeDollarSignIcon aria-hidden /></Button>
        <Button size="icon-xs" variant="ghost" aria-label={`${t("common.actions.edit")}: ${model.model_id}`} onClick={() => openEditor(model)}><PencilIcon aria-hidden /></Button>
        <EntityDeleteButton entity="provider-models" id={model.id} label={model.model_id} queryKeys={["provider-models"]} />
      </div>
    ) },
  ]

  return (
    <Section
      title={t("providers.models.title")}
      description={t("providers.models.description")}
      actions={<Button size="sm" onClick={() => openEditor()}>{t("providers.models.add")}</Button>}
    >
      <DataTable
        columns={columns}
        rows={rows}
        rowKey={(model) => model.id}
        searchText={(model) => `${model.model_id} ${model.display_name ?? ""}`}
        renderCard={(model) => <div className="flex items-center justify-between gap-3"><div className="min-w-0"><p className="machine-text truncate text-xs">{model.model_id}</p><p className="text-xs text-muted-foreground">{number(model.context_window)} · {number(model.max_output_tokens)}</p></div><Switch checked={model.enabled} aria-label={model.model_id} onCheckedChange={(enabled) => mutation.mutate({ value: { ...request(model), enabled }, id: model.id })} /></div>}
        empty={t("providers.models.empty")}
        storageKey="provider-models"
        onRowClick={openEditor}
      />
      <ProviderModelDialog
        key={editing?.id ?? "new"}
        open={open}
        onOpenChange={(value) => { setOpen(value); if (!value) setEditing(undefined) }}
        providerId={providerId}
        model={editing}
        saving={mutation.isPending}
        onSave={async (value, id) => { await mutation.mutateAsync({ value, id }) }}
      />
    </Section>
  )
}

function request(model: ProviderModelDto): ProviderModelWriteRequest {
  return {
    provider_id: model.provider_id,
    model_id: model.model_id,
    display_name: model.display_name,
    variants: model.variants,
    context_window: model.context_window,
    max_output_tokens: model.max_output_tokens,
    thinking_supported: model.thinking_supported,
    thinking_adaptive_supported: model.thinking_adaptive_supported,
    thinking_enabled_supported: model.thinking_enabled_supported,
    enabled: model.enabled,
  }
}
