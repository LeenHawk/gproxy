import { useState } from "react"
import { useMutation, useQueryClient } from "@tanstack/react-query"
import { PencilIcon, Trash2Icon } from "lucide-react"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import { deleteRoutingRule, resetRoutingDefaults, saveRoutingRule } from "@/api/control"
import type { ChannelDto } from "@/generated/ChannelDto"
import type { ProviderDto } from "@/generated/ProviderDto"
import type { RoutingRuleDto } from "@/generated/RoutingRuleDto"
import type { RoutingRuleWriteRequest } from "@/generated/RoutingRuleWriteRequest"
import { ConfirmDangerous } from "@/components/confirm-dangerous"
import { RoutingBehaviorBadge } from "@/components/rules/routing-behavior-badge"
import { RoutingRuleDialog } from "@/components/rules/routing-rule-dialog"
import { Button } from "@/components/ui/button"
import { Card, CardAction, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Empty, EmptyContent, EmptyHeader, EmptyTitle } from "@/components/ui/empty"
import { Switch } from "@/components/ui/switch"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import { cn } from "@/lib/utils"

function writeRequest(rule: RoutingRuleDto, enabled = rule.enabled): RoutingRuleWriteRequest {
  return {
    provider_id: rule.provider_id,
    operation: rule.operation,
    kind: rule.kind,
    implementation: rule.implementation,
    dest_operation: rule.dest_operation,
    dest_kind: rule.dest_kind,
    sort_order: rule.sort_order,
    enabled,
  }
}

export function ProviderRoutingRules({ provider, channel, rules }: { provider: ProviderDto; channel?: ChannelDto; rules: Array<RoutingRuleDto> }) {
  const { t } = useTranslation()
  const client = useQueryClient()
  const [editorOpen, setEditorOpen] = useState(false)
  const [editorRule, setEditorRule] = useState<RoutingRuleDto>()
  const [deleteTarget, setDeleteTarget] = useState<RoutingRuleDto>()
  const [resetConfirm, setResetConfirm] = useState(false)
  const refresh = () => client.invalidateQueries({ queryKey: ["routing-rules"] })
  const saved = () => toast.success(t("rules.saved"))
  const failed = () => toast.error(t("rules.saveError"))
  const mutation = useMutation({ mutationFn: ({ value, id }: { value: RoutingRuleWriteRequest; id?: number }) => saveRoutingRule(value, id), onSuccess: async () => { await refresh(); saved() }, onError: failed })
  const remove = useMutation({ mutationFn: deleteRoutingRule, onSuccess: async () => { await refresh(); setDeleteTarget(undefined); saved() }, onError: () => { setDeleteTarget(undefined); failed() } })
  const reset = useMutation({ mutationFn: () => resetRoutingDefaults(provider.id), onSuccess: async () => { await refresh(); setResetConfirm(false); saved() }, onError: () => { setResetConfirm(false); failed() } })
  const list = rules.filter((rule) => rule.provider_id === provider.id)
  const openEditor = (rule?: RoutingRuleDto) => { setEditorRule(rule); setEditorOpen(true) }

  return (
    <Card>
      <CardHeader className="!grid-cols-1 sm:!grid-cols-[1fr_auto]">
        <CardTitle>{t("rules.routing.title")}</CardTitle>
        <CardDescription>{t("rules.routing.description")}</CardDescription>
        <CardAction className="!col-start-1 !row-span-1 !row-start-3 flex !justify-self-start gap-2 sm:!col-start-2 sm:!row-span-2 sm:!row-start-1 sm:!justify-self-end">
          <Button size="sm" variant="outline" disabled={reset.isPending} onClick={() => setResetConfirm(true)}>{t("rules.routing.reset")}</Button>
          <Button size="sm" onClick={() => openEditor()}>{t("rules.routing.add")}</Button>
        </CardAction>
      </CardHeader>
      <CardContent>
        {list.length === 0 ? (
          <Empty className="border border-dashed py-10">
            <EmptyHeader><EmptyTitle>{t("rules.routing.empty")}</EmptyTitle></EmptyHeader>
            <EmptyContent><Button disabled={reset.isPending} onClick={() => reset.mutate()}>{t("rules.routing.initialize")}</Button></EmptyContent>
          </Empty>
        ) : (
          <div className="rounded-md border">
            <Table className="min-w-[40rem] table-fixed">
              <TableHeader><TableRow>
                <TableHead className="w-36">{t("rules.routing.columns.operation")}</TableHead>
                <TableHead className="w-36">{t("rules.routing.columns.kind")}</TableHead>
                <TableHead>{t("rules.routing.columns.behavior")}</TableHead>
                <TableHead className="w-16">{t("rules.fields.enabled")}</TableHead>
                <TableHead className="w-16"><span className="sr-only">{t("common.actions.edit")} / {t("common.actions.delete")}</span></TableHead>
              </TableRow></TableHeader>
              <TableBody>{list.map((rule, index) => (
                <TableRow
                  key={rule.id}
                  tabIndex={0}
                  title={rule.inherited ? t("rules.values.inherited") : undefined}
                  onClick={() => openEditor(rule)}
                  onKeyDown={(event) => { if (event.target === event.currentTarget && (event.key === "Enter" || event.key === " ")) { event.preventDefault(); openEditor(rule) } }}
                  className={cn("cursor-pointer hover:bg-accent/50", index % 2 === 0 ? "bg-background" : "bg-muted/20", rule.inherited && "text-muted-foreground")}
                >
                  <TableCell className={cn("whitespace-normal [overflow-wrap:anywhere]", !rule.inherited && "font-medium")}>{t(`rules.operations.${rule.operation}`, { defaultValue: rule.operation })}</TableCell>
                  <TableCell className="whitespace-normal [overflow-wrap:anywhere]">{t(`rules.wires.${rule.kind}`, { defaultValue: rule.kind })}</TableCell>
                  <TableCell className="whitespace-normal"><RoutingBehaviorBadge rule={rule} /></TableCell>
                  <TableCell onClick={(event) => event.stopPropagation()}>
                    <Switch
                      checked={rule.enabled}
                      disabled={mutation.isPending}
                      aria-label={`${t("rules.fields.enabled")}: ${rule.operation} · ${rule.kind}`}
                      onCheckedChange={(enabled) => mutation.mutate({ value: writeRequest(rule, enabled), id: rule.id })}
                    />
                  </TableCell>
                  <TableCell onClick={(event) => event.stopPropagation()}>
                    <div className="flex items-center justify-end gap-1">
                      <Button size="icon-xs" variant="ghost" aria-label={t("common.actions.edit")} onClick={() => openEditor(rule)}><PencilIcon aria-hidden /></Button>
                      {!rule.inherited ? <Button size="icon-xs" variant="destructive" aria-label={t("rules.routing.delete")} onClick={() => setDeleteTarget(rule)}><Trash2Icon aria-hidden /></Button> : null}
                    </div>
                  </TableCell>
                </TableRow>
              ))}</TableBody>
            </Table>
          </div>
        )}
      </CardContent>
      <RoutingRuleDialog
        key={editorRule?.id ?? "new"}
        open={editorOpen}
        onOpenChange={(open) => { setEditorOpen(open); if (!open) setEditorRule(undefined) }}
        providerId={provider.id}
        channel={channel}
        rule={editorRule}
        saving={mutation.isPending}
        onSave={async (value, id) => { await mutation.mutateAsync({ value, id }) }}
      />
      <ConfirmDangerous open={deleteTarget !== undefined} onOpenChange={(open) => { if (!open) setDeleteTarget(undefined) }} title={t("rules.routing.delete")} description={t("rules.routing.deleteConfirm")} confirmLabel={t("rules.routing.delete")} pending={remove.isPending} onConfirm={() => { if (deleteTarget) remove.mutate(deleteTarget.id) }} />
      <ConfirmDangerous open={resetConfirm} onOpenChange={setResetConfirm} title={t("rules.routing.reset")} description={t("rules.routing.resetConfirm")} confirmLabel={t("rules.routing.reset")} pending={reset.isPending} onConfirm={() => reset.mutate()} />
    </Card>
  )
}
