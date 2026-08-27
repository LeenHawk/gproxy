import { useMutation, useQueryClient } from "@tanstack/react-query"
import { PencilIcon, RotateCcwIcon, Trash2Icon } from "lucide-react"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import { deleteRoutingRule, resetRoutingDefaults, saveRoutingRule } from "@/api/control"
import type { ChannelDto } from "@/generated/ChannelDto"
import type { ChannelSupportDto } from "@/generated/ChannelSupportDto"
import type { ProviderDto } from "@/generated/ProviderDto"
import type { RoutingRuleDto } from "@/generated/RoutingRuleDto"
import type { RoutingRuleWriteRequest } from "@/generated/RoutingRuleWriteRequest"
import { RoutingRuleDialog } from "@/components/rules/routing-rule-dialog"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Card, CardAction, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"

export function ProviderRoutingRules({ provider, channel, rules }: { provider: ProviderDto; channel?: ChannelDto; rules: Array<RoutingRuleDto> }) {
  const { t } = useTranslation()
  const client = useQueryClient()
  const refresh = () => client.invalidateQueries({ queryKey: ["routing-rules"] })
  const saved = () => toast.success(t("rules.saved"))
  const failed = () => toast.error(t("rules.saveError"))
  const mutation = useMutation({ mutationFn: ({ value, id }: { value: RoutingRuleWriteRequest; id?: number }) => saveRoutingRule(value, id), onSuccess: async () => { await refresh(); saved() }, onError: failed })
  const remove = useMutation({ mutationFn: deleteRoutingRule, onSuccess: refresh, onError: failed })
  const reset = useMutation({ mutationFn: () => resetRoutingDefaults(provider.id), onSuccess: async () => { await refresh(); saved() }, onError: failed })
  const supports = channel?.supports ?? []
  const explicit = rules.filter((rule) => rule.provider_id === provider.id)
  const operations = [...new Set(supports.map((support) => support.operation))]
  const kinds = [...new Set(supports.map((support) => support.source))]
  const supportAt = (operation: string, kind: string) => supports.find((support) => support.operation === operation && support.source === kind)
  const ruleAt = (operation: string, kind: string) => explicit.find((rule) => rule.operation === operation && rule.kind === kind)

  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("rules.routing.title")}</CardTitle>
        <CardDescription>{t("rules.routing.description")}</CardDescription>
        <CardAction><Button size="sm" variant="outline" disabled={reset.isPending} onClick={() => reset.mutate()}><RotateCcwIcon data-icon="inline-start" />{t("rules.routing.reset")}</Button></CardAction>
      </CardHeader>
      <CardContent>
        <Table>
          <TableHeader><TableRow><TableHead>{t("rules.fields.operation")}</TableHead>{kinds.map((kind) => <TableHead key={kind}>{t(`rules.wires.${kind}`, { defaultValue: kind })}</TableHead>)}</TableRow></TableHeader>
          <TableBody>{operations.map((operation) => <TableRow key={operation}><TableCell className="font-medium">{t(`rules.operations.${operation}`, { defaultValue: operation })}</TableCell>{kinds.map((kind) => <TableCell key={kind}>{cell(supportAt(operation, kind), ruleAt(operation, kind))}</TableCell>)}</TableRow>)}</TableBody>
        </Table>
      </CardContent>
    </Card>
  )

  function cell(support: ChannelSupportDto | undefined, rule: RoutingRuleDto | undefined) {
    if (!support) return <span className="text-muted-foreground">—</span>
    const implementation = rule?.implementation ?? support.implementation
    const inherited = rule?.inherited ?? true
    const defaults = { operation: support.operation, kind: support.source, implementation: support.implementation, destOperation: support.target_operation, destKind: support.target }
    return <div className="flex min-w-40 items-center justify-between gap-2"><div className="flex flex-col items-start gap-1"><Badge variant="outline">{t(`rules.values.${implementation}`)}</Badge>{inherited ? <Badge variant="secondary">{t("rules.values.inherited")}</Badge> : null}</div><div className="flex items-center gap-1"><RoutingRuleDialog providerId={provider.id} channel={channel} rule={rule} defaults={defaults} saving={mutation.isPending} onSave={async (value, id) => { await mutation.mutateAsync({ value, id }) }} trigger={<Button size="icon-xs" variant="outline" aria-label={t("common.actions.edit")}><PencilIcon aria-hidden /></Button>} />{rule && !rule.inherited ? <Button size="icon-xs" variant="ghost" aria-label={t("common.actions.delete")} onClick={() => remove.mutate(rule.id)}><Trash2Icon aria-hidden /></Button> : null}</div></div>
  }
}
