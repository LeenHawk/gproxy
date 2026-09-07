import { ModelCapabilities } from "./model-capabilities"
import { useTranslation } from "react-i18next"
import type { PortalModelDto } from "@/generated/PortalModelDto"
import { DataTable, type DataTableColumn } from "@/components/data-table"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"

export function ModelCatalog({ models }: { models: Array<PortalModelDto> }) {
  const { t } = useTranslation()
  const capabilities = (model: PortalModelDto) => <ModelCapabilities model={model} />
  const columns: Array<DataTableColumn<PortalModelDto>> = [
    { key: "model", label: t("portal.models.model"), header: t("portal.models.model"), cell: (model) => <span className="font-mono text-xs">{model.name}</span> },
    { key: "capabilities", label: t("portal.models.capabilities"), header: t("portal.models.capabilities"), cell: capabilities },
  ]

  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("portal.models.title")}</CardTitle>
        <CardDescription>{t("portal.models.description")}</CardDescription>
      </CardHeader>
      <CardContent>
        <DataTable columns={columns} rows={models} rowKey={(model) => model.name} searchText={(model) => `${model.name} ${model.capabilities.map((capability) => `${capability.group} ${capability.source} ${capability.operation}`).join(" ")}`} renderCard={(model) => <div className="flex flex-col gap-3"><p className="font-mono text-xs">{model.name}</p>{capabilities(model)}</div>} empty={t("portal.models.empty")} storageKey="portal-models" />
      </CardContent>
    </Card>
  )
}
