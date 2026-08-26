import { useTranslation } from "react-i18next"
import { ArrowRightIcon, CableIcon } from "lucide-react"
import type { ChannelDto } from "@/generated/ChannelDto"
import { DataTable, type DataTableColumn } from "@/components/data-table"
import { Badge } from "@/components/ui/badge"

type ChannelRow = { id: string; channel: ChannelDto; support: ChannelDto["supports"][number] | null }

export function ChannelCatalog({ channels }: { channels: Array<ChannelDto> }) {
  const { t } = useTranslation()
  const rows = channels.flatMap<ChannelRow>((channel) => channel.supports.length === 0
    ? [{ id: `${channel.id}-empty`, channel, support: null }]
    : channel.supports.map((support, index) => ({ id: `${channel.id}-${index}`, channel, support })))
  const columns: Array<DataTableColumn<ChannelRow>> = [
    { key: "channel", label: t("channels.channel"), header: t("channels.channel"), cell: ({ channel }) => <div><p className="flex items-center gap-2 font-medium"><CableIcon aria-hidden />{channel.display_name}</p><p className="font-mono text-xs text-muted-foreground">{channel.id}</p></div> },
    { key: "group", label: t("channels.group"), header: t("channels.group"), cell: ({ support }) => support ? <Badge variant="outline"><span className="font-mono text-xs">{support.group}</span></Badge> : t("channels.noOperations") },
    { key: "operation", label: t("channels.operation"), header: t("channels.operation"), cell: ({ support }) => <span className="font-mono text-xs">{support?.operation ?? t("common.none")}</span> },
    { key: "source", label: t("channels.source"), header: t("channels.source"), cell: ({ support }) => <span className="font-mono text-xs">{support?.source ?? t("common.none")}</span> },
    { key: "target", label: t("channels.target"), header: t("channels.target"), cell: ({ support }) => <span className="inline-flex items-center gap-1.5 font-mono text-xs">{support ? <ArrowRightIcon className="text-muted-foreground" aria-hidden /> : null}{support?.target ?? t("common.none")}</span> },
  ]
  return (
    <DataTable columns={columns} rows={rows} rowKey={(row) => row.id} searchText={({ channel, support }) => `${channel.display_name} ${channel.id} ${support?.group ?? ""} ${support?.operation ?? ""} ${support?.source ?? ""} ${support?.target ?? ""}`} renderCard={({ channel, support }) => <div><p className="flex items-center gap-2 font-medium"><CableIcon aria-hidden />{channel.display_name}</p><p className="font-mono text-xs text-muted-foreground">{channel.id}</p>{support ? <div className="mt-3 flex flex-wrap items-center gap-2"><Badge variant="outline">{support.group}</Badge><span className="font-mono text-xs">{support.source}</span><ArrowRightIcon aria-hidden /><span className="font-mono text-xs">{support.target}</span></div> : <p className="mt-2 text-xs text-muted-foreground">{t("channels.noOperations")}</p>}</div>} empty={t("channels.noOperations")} storageKey="channels" />
  )
}
