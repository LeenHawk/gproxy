import { PencilIcon, PlusIcon, Trash2Icon } from "lucide-react"
import { useMutation, useQueryClient } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import type { ChannelDto } from "@/generated/ChannelDto"
import type { ProviderDto } from "@/generated/ProviderDto"
import type { RoutingRuleDto } from "@/generated/RoutingRuleDto"
import type { RoutingRuleWriteRequest } from "@/generated/RoutingRuleWriteRequest"
import { deleteRoutingRule, saveRoutingRule } from "@/api/control"
import { RoutingRuleDialog } from "./routing-rule-dialog"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"

export function ProviderRoutingRules({ provider, channel, rules }: { provider: ProviderDto; channel?: ChannelDto; rules: Array<RoutingRuleDto> }) {
  const { t } = useTranslation()
  const client = useQueryClient()
  const refresh = () => client.invalidateQueries({ queryKey: ["routing-rules"] })
  const saved = () => toast.success(t("rules.saved"))
  const failed = () => toast.error(t("rules.saveError"))
  const mutation = useMutation({ mutationFn: ({ value, id }: { value: RoutingRuleWriteRequest; id?: number }) => saveRoutingRule(value, id), onSuccess: async () => { await refresh(); saved() }, onError: failed })
  const remove = useMutation({ mutationFn: deleteRoutingRule, onSuccess: refresh, onError: failed })
  const explicit = rules.filter((rule) => rule.provider_id === provider.id)
  const supports = channel?.supports ?? []
  return <Card><CardHeader><CardTitle>{t("rules.routing.title")}</CardTitle><CardDescription>{t("rules.routing.description")}</CardDescription></CardHeader><CardContent className="flex flex-col gap-3">
    {supports.map((support) => {
      const rule = explicit.find((value) => value.operation === support.operation && value.kind === support.source)
      const defaults = { operation: support.operation, kind: support.source, destOperation: support.target_operation, destKind: support.target }
      const implementation = rule?.implementation ?? (support.source === support.target && support.operation === support.target_operation ? "passthrough" : "transform_to")
      const detail = implementation === "transform_to" ? t("rules.routing.transformSummary", { implementation: t("rules.values.transform_to"), destination: t(`rules.wires.${rule?.dest_kind ?? support.target}`, { defaultValue: rule?.dest_kind ?? support.target }) }) : t(`rules.values.${implementation}`)
      return <div key={`${support.operation}:${support.source}`} className="flex flex-col gap-3 rounded-lg border p-3 sm:flex-row sm:items-center sm:justify-between"><div className="min-w-0"><div className="flex flex-wrap items-center gap-2"><Badge variant="outline">{t(`rules.operations.${support.operation}`, { defaultValue: support.operation })}</Badge><Badge variant="secondary">{t(`rules.wires.${support.source}`, { defaultValue: support.source })}</Badge>{rule ? null : <Badge variant="secondary">{t("rules.values.inherited")}</Badge>}</div><p className="mt-2 text-sm">{detail}</p></div><div className="flex justify-end gap-2"><RoutingRuleDialog providerId={provider.id} channel={channel} rule={rule} defaults={defaults} saving={mutation.isPending} onSave={async (value, id) => { await mutation.mutateAsync({ value, id }) }} trigger={<Button size="icon-sm" variant="outline" aria-label={t(rule ? "common.actions.edit" : "rules.routing.override")} >{rule ? <PencilIcon aria-hidden /> : <PlusIcon aria-hidden />}</Button>} />{rule ? <Button size="icon-sm" variant="ghost" aria-label={t("common.actions.delete")} onClick={() => remove.mutate(rule.id)}><Trash2Icon aria-hidden /></Button> : null}</div></div>
    })}
  </CardContent></Card>
}
