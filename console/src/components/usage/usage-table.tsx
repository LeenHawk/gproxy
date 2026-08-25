import { useMemo } from "react"
import { useTranslation } from "react-i18next"
import type { UsageAggregateDto } from "@/generated/UsageAggregateDto"
import type { UsageGroupByDto } from "@/generated/UsageGroupByDto"
import type { ProviderDto } from "@/generated/ProviderDto"
import type { UserDto } from "@/generated/UserDto"
import type { UserKeyDto } from "@/generated/UserKeyDto"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import { Empty, EmptyHeader, EmptyTitle } from "@/components/ui/empty"
import { formatCost, formatCount } from "@/lib/format"
import { cn } from "@/lib/utils"

export function UsageTable({ rows, group, providers, users, keys }: { rows: Array<UsageAggregateDto>; group: UsageGroupByDto; providers: Array<ProviderDto>; users: Array<UserDto>; keys: Array<UserKeyDto> }) {
  const { t, i18n } = useTranslation()
  const providerNames = useMemo(() => new Map(providers.map((provider) => [provider.id, provider.name])), [providers])
  const userNames = useMemo(() => new Map(users.map((user) => [user.id, user.name])), [users])
  const keyNames = useMemo(() => new Map(keys.map((key) => [key.id, key.label ?? key.prefix ?? String(key.id)])), [keys])
  const label = (value: string) => {
    const id = Number(value)
    if (group === "provider") return providerNames.get(id) ?? value
    if (group === "user") return userNames.get(id) ?? value
    if (group === "user_key") return keyNames.get(id) ?? value
    return value
  }
  if (rows.length === 0) {
    return <Empty><EmptyHeader><EmptyTitle>{t("usage.empty")}</EmptyTitle></EmptyHeader></Empty>
  }
  return (
    <div className="overflow-hidden rounded-md border bg-card">
      <Table>
        <TableHeader><TableRow><TableHead>{t("usage.group")}</TableHead><TableHead className="text-right">{t("usage.requests")}</TableHead><TableHead className="text-right">{t("usage.inputTokens")}</TableHead><TableHead className="text-right">{t("usage.cachedTokens")}</TableHead><TableHead className="text-right">{t("usage.outputTokens")}</TableHead><TableHead className="text-right">{t("usage.cost.label")}</TableHead></TableRow></TableHeader>
        <TableBody>
          {rows.map((row) => {
            const groupLabel = label(row.group)
            return <TableRow key={row.group}>
              <TableCell><div className={cn("text-sm", group === "model" && "font-mono text-xs")}>{groupLabel}</div>{groupLabel !== row.group ? <div className="font-mono text-xs text-muted-foreground">{row.group}</div> : null}</TableCell>
              <TableCell className="text-right font-mono text-xs">{formatCount(row.requests, i18n.language)}</TableCell>
              <TableCell className="text-right font-mono text-xs">{formatCount(row.input_tokens, i18n.language)}</TableCell>
              <TableCell className="text-right font-mono text-xs">{formatCount(row.cached_input_tokens, i18n.language)}</TableCell>
              <TableCell className="text-right font-mono text-xs">{formatCount(row.output_tokens, i18n.language)}</TableCell>
              <TableCell className="text-right font-mono text-xs">{formatCost(row.cost, i18n.language)}</TableCell>
            </TableRow>
          })}
        </TableBody>
      </Table>
    </div>
  )
}
