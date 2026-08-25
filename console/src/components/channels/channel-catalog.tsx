import { useTranslation } from "react-i18next"
import { ArrowRightIcon } from "lucide-react"
import type { ChannelDto } from "@/generated/ChannelDto"
import { Badge } from "@/components/ui/badge"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"

export function ChannelCatalog({ channels }: { channels: Array<ChannelDto> }) {
  const { t } = useTranslation()
  return (
    <div className="overflow-hidden rounded-md border bg-card">
      <Table>
        <TableHeader><TableRow><TableHead>{t("channels.channel")}</TableHead><TableHead>{t("channels.group")}</TableHead><TableHead>{t("channels.operation")}</TableHead><TableHead>{t("channels.source")}</TableHead><TableHead>{t("channels.target")}</TableHead></TableRow></TableHeader>
        <TableBody>
          {channels.flatMap((channel) => channel.supports.length === 0 ? [
            <TableRow key={`${channel.id}-empty`}><TableCell><div className="font-medium">{channel.display_name}</div><div className="font-mono text-xs text-muted-foreground">{channel.id}</div></TableCell><TableCell colSpan={4} className="text-muted-foreground">{t("channels.noOperations")}</TableCell></TableRow>,
          ] : channel.supports.map((support, index) => (
            <TableRow key={`${channel.id}-${support.source}-${support.target}-${support.operation}`}>
              <TableCell>{index === 0 ? <><div className="font-medium">{channel.display_name}</div><div className="font-mono text-xs text-muted-foreground">{channel.id}</div></> : null}</TableCell>
              <TableCell><Badge variant="outline"><span className="font-mono text-xs">{support.group}</span></Badge></TableCell>
              <TableCell className="font-mono text-xs">{support.operation}</TableCell>
              <TableCell className="font-mono text-xs">{support.source}</TableCell>
              <TableCell><span className="inline-flex items-center gap-1.5 font-mono text-xs"><ArrowRightIcon className="text-muted-foreground" />{support.target}</span></TableCell>
            </TableRow>
          )))}
        </TableBody>
      </Table>
    </div>
  )
}
