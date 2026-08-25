import { useTranslation } from "react-i18next"
import type { PortalModelDto } from "@/generated/PortalModelDto"
import { Badge } from "@/components/ui/badge"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Empty, EmptyDescription, EmptyHeader, EmptyTitle } from "@/components/ui/empty"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"

export function ModelCatalog({ models }: { models: Array<PortalModelDto> }) {
  const { t } = useTranslation()

  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("portal.models.title")}</CardTitle>
        <CardDescription>{t("portal.models.description")}</CardDescription>
      </CardHeader>
      <CardContent>
        {models.length === 0 ? (
          <Empty>
            <EmptyHeader>
              <EmptyTitle>{t("portal.models.empty")}</EmptyTitle>
              <EmptyDescription>{t("portal.models.emptyDescription")}</EmptyDescription>
            </EmptyHeader>
          </Empty>
        ) : (
          <Table>
            <TableHeader><TableRow>
              <TableHead>{t("portal.models.model")}</TableHead>
              <TableHead>{t("portal.models.capabilities")}</TableHead>
            </TableRow></TableHeader>
            <TableBody>{models.map((model) => (
              <TableRow key={model.name}>
                <TableCell className="font-mono text-xs">{model.name}</TableCell>
                <TableCell>
                  <ul className="flex min-w-80 flex-col gap-2">
                    {model.capabilities.map((capability) => (
                      <li
                        key={`${capability.source}:${capability.operation}:${capability.group}`}
                        className="flex flex-wrap items-center gap-2"
                      >
                        <Badge variant="outline"><code className="font-mono">{capability.group}</code></Badge>
                        <code className="font-mono text-xs">{capability.source}</code>
                        <code className="font-mono text-xs text-muted-foreground">{capability.operation}</code>
                      </li>
                    ))}
                  </ul>
                </TableCell>
              </TableRow>
            ))}</TableBody>
          </Table>
        )}
      </CardContent>
    </Card>
  )
}
