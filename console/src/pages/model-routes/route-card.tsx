import { useState } from "react"
import { useMutation, useQueryClient } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import { ArrowDown, ArrowRight, ArrowUp, ChevronRight, Trash2 } from "lucide-react"
import type { ProviderDto, RouteDto, RouteMemberDto } from "@/generated/sdk"
import { ConfirmButton } from "@/components/confirm"
import { PageSection } from "@/components/page"
import { ErrorNotice } from "@/components/state"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Field, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Switch } from "@/components/ui/switch"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
import { cn } from "@/lib/utils"
import { routeFamily, memberFamily } from "./families"
import { MemberAdder } from "./member-adder"
import { STRATEGIES, draftOf, memberBatch, newKey, normalize, routePatch, sameDraft, shareOf, tiersOf, type Draft, type DraftMember, type Strategy } from "./draft"

export type RouteContext = {
  providers: Map<string, ProviderDto>
  /** The instance-wide attempt ceiling; a route can ask for fewer, never more. */
  attemptCeiling: number
}

const providerLabel = (provider: ProviderDto | undefined, id: string) => provider ? provider.displayName ?? provider.name : id

export function RouteCard({ route, members, context, open, onOpenChange }: { route: RouteDto | null; members: RouteMemberDto[]; context: RouteContext; open: boolean; onOpenChange: (open: boolean) => void }) {
  const { t } = useTranslation()
  const client = useQueryClient()
  const enabled = useMutation({
    mutationFn: (value: boolean) => routeFamily.update(route!.id, { enabled: value }),
    onSuccess: () => client.invalidateQueries({ queryKey: ["admin", routeFamily.path] }),
    onError: (error: Error) => toast.error(error.message),
  })
  if (!route) return <div className="rounded-lg border border-primary/40"><RouteEditor route={null} members={[]} context={context} onDone={() => onOpenChange(false)} /></div>
  const layers = tiersOf(draftOf(route, members, context.attemptCeiling).members)
  return <div className={cn("rounded-lg border", !route.enabled && "bg-muted/30")}>
    <div className="flex items-start gap-3 p-3">
      <button type="button" className="flex min-w-0 flex-1 items-start gap-2 text-left" aria-expanded={open} onClick={() => onOpenChange(!open)}>
        <ChevronRight className={cn("mt-0.5 size-4 shrink-0 text-muted-foreground transition-transform", open && "rotate-90")} />
        <span className="flex min-w-0 flex-col gap-1">
          <span className="flex flex-wrap items-center gap-2">
            <span className={cn("font-mono font-medium break-all", !route.enabled && "text-muted-foreground")}>{route.name}</span>
            <Badge variant="outline">{t(`values.${route.strategy}`, { defaultValue: route.strategy })}</Badge>
            {route.sessionAffinity ? <Badge variant="outline">{t("modelRoutes.sticky")}</Badge> : null}
          </span>
          <span className="text-sm text-muted-foreground wrap-anywhere">
            {layers.length ? layers.map((layer, index) => <span key={index}>
              {index ? <ArrowRight className="mx-1 inline size-3.5 align-[-2px]" aria-label={t("modelRoutes.thenTier")} /> : null}
              {layer.map((member, position) => <span key={member.key}>{position ? " · " : ""}<span className={cn(!usable(member, context) && "line-through opacity-60")}>{providerLabel(context.providers.get(member.providerId), member.providerId)} <span className="font-mono text-xs">{member.upstreamModel}</span></span></span>)}
            </span>) : t("modelRoutes.noMembers")}
          </span>
        </span>
      </button>
      <Switch checked={route.enabled} disabled={enabled.isPending} aria-label={t("modelRoutes.routeEnabled", { name: route.name })} onCheckedChange={value => enabled.mutate(value)} />
    </div>
    {open ? <div className="border-t"><RouteEditor route={route} members={members} context={context} onDone={() => onOpenChange(false)} /></div> : null}
  </div>
}

const usable = (member: DraftMember, context: RouteContext) => member.enabled && !!context.providers.get(member.providerId)?.enabled

function RouteEditor({ route, members, context, onDone }: { route: RouteDto | null; members: RouteMemberDto[]; context: RouteContext; onDone: () => void }) {
  const { t } = useTranslation()
  const client = useQueryClient()
  const original = draftOf(route, members, context.attemptCeiling)
  // Null until the first edit, so a refetch after saving shows the saved rows.
  const [edited, setDraft] = useState<Draft | null>(null)
  const draft = edited ?? original
  const [addTier, setAddTier] = useState(0)
  const dirty = !sameDraft(draft, original)
  const set = (patch: Partial<Draft>) => setDraft({ ...draft, ...patch })
  const setMembers = (next: DraftMember[]) => setDraft({ ...draft, members: normalize(next) })
  const editMember = (key: string, patch: Partial<DraftMember>) => setMembers(draft.members.map(member => member.key === key ? { ...member, ...patch } : member))
  const layers = tiersOf(draft.members)

  const save = useMutation({
    mutationFn: async () => {
      let routeId = route?.id
      if (!route) routeId = (await routeFamily.create({ name: draft.name.trim(), strategy: draft.strategy, sessionAffinity: draft.sessionAffinity, maxAttempts: draft.maxAttempts, enabled: draft.enabled })).id
      else { const patch = routePatch(route, draft); if (patch) await routeFamily.update(route.id, patch) }
      const items = memberBatch(routeId!, members, draft)
      if (items.length) await memberFamily.batch(items)
    },
    onSuccess: async () => {
      await Promise.all([client.invalidateQueries({ queryKey: ["admin", routeFamily.path] }), client.invalidateQueries({ queryKey: ["admin", memberFamily.path] })])
      setDraft(null)
      toast.success(t(route ? "toast.saved" : "toast.created"))
      if (!route) onDone()
    },
  })
  const remove = useMutation({
    mutationFn: () => routeFamily.remove(route!.id),
    onSuccess: async () => {
      await Promise.all([client.invalidateQueries({ queryKey: ["admin", routeFamily.path] }), client.invalidateQueries({ queryKey: ["admin", memberFamily.path] })])
      toast.success(t("toast.deleted"))
    },
    onError: (error: Error) => toast.error(error.message),
  })

  const idPrefix = `route-${route?.id ?? "new"}`

  return <div className="flex flex-col gap-6 p-4">
    <div className="grid gap-4 md:grid-cols-2">
      <Field>
        <FieldLabel htmlFor={`${idPrefix}-name`}>{t("modelRoutes.modelName")}</FieldLabel>
        <Input id={`${idPrefix}-name`} className="font-mono" value={draft.name} placeholder="claude-opus-5-5" onChange={event => set({ name: event.target.value })} />
      </Field>
      <Field>
        <FieldLabel htmlFor={`${idPrefix}-attempts`}>{t("fields.maxAttempts")}</FieldLabel>
        <Input id={`${idPrefix}-attempts`} className="w-28" type="number" min={1} max={context.attemptCeiling} value={draft.maxAttempts} onChange={event => set({ maxAttempts: Math.min(context.attemptCeiling, Math.max(1, Math.round(Number(event.target.value)) || 1)) })} />
      </Field>
    </div>

    <div className="flex flex-wrap items-center justify-between gap-x-6 gap-y-3">
      <div className="flex flex-wrap items-center gap-3">
        <FieldLabel>{t("fields.strategy")}</FieldLabel>
        <ToggleGroup type="single" variant="outline" size="sm" className="flex-wrap" value={draft.strategy} onValueChange={value => { if (value) set({ strategy: value as Strategy }) }} aria-label={t("fields.strategy")}>
          {STRATEGIES.map(strategy => <ToggleGroupItem key={strategy} value={strategy}>{t(`values.${strategy}`)}</ToggleGroupItem>)}
        </ToggleGroup>
      </div>
      <div className="flex items-center gap-2">
        <Switch id={`${idPrefix}-sticky`} checked={draft.sessionAffinity} onCheckedChange={value => set({ sessionAffinity: value })} />
        <FieldLabel htmlFor={`${idPrefix}-sticky`}>{t("modelRoutes.sticky")}</FieldLabel>
      </div>
    </div>

    <PageSection title={t("modelRoutes.members")}>
      <div className="flex flex-col gap-3">
        {layers.length ? layers.map((layer, tier) => <div key={tier} className="flex flex-col gap-1.5">
          <span className="text-xs font-medium">{t("modelRoutes.tierTitle", { n: tier + 1 })}</span>
          <ul className="flex flex-col divide-y rounded-lg border">
            {layer.map(member => {
              const provider = context.providers.get(member.providerId)
              const share = draft.strategy === "weighted" ? shareOf(member, layer, candidate => usable(candidate, context)) : null
              return <li key={member.key} className={cn("flex flex-wrap items-center gap-x-3 gap-y-2 p-2.5 text-sm", !usable(member, context) && "bg-muted/40")}>
                <Switch checked={member.enabled} aria-label={t("modelRoutes.memberEnabled", { name: member.upstreamModel })} onCheckedChange={value => editMember(member.key, { enabled: value })} />
                <span className="flex min-w-48 flex-1 flex-col">
                  <span className="flex flex-wrap items-center gap-2">
                    <span className={cn("truncate", !usable(member, context) && "text-muted-foreground")}>{providerLabel(provider, member.providerId)}</span>
                    <span className="truncate font-mono text-xs text-muted-foreground">{member.upstreamModel}</span>
                    {!provider ? <Badge variant="destructive">{t("modelRoutes.providerMissing")}</Badge> : !provider.enabled ? <Badge variant="warning">{t("modelRoutes.providerDisabled")}</Badge> : null}
                    {!member.id ? <Badge variant="info">{t("modelRoutes.unsaved")}</Badge> : null}
                  </span>
                </span>
                {draft.strategy !== "round_robin" ? <label className="flex items-center gap-1.5 text-xs text-muted-foreground" title={t(draft.strategy === "weighted" ? "modelRoutes.weightHintWeighted" : "modelRoutes.weightHintFailover")}>
                  {t("fields.weight")}
                  <Input type="number" min={1} className="h-7 w-16" value={member.weight} onChange={event => editMember(member.key, { weight: Math.max(1, Math.round(Number(event.target.value)) || 1) })} />
                  {share != null ? <span className="w-9 tabular-nums">{share}%</span> : null}
                </label> : null}
                <span className="flex items-center">
                  <Button type="button" size="icon-sm" variant="ghost" disabled={tier === 0 && layer.length === 1} aria-label={t("modelRoutes.moveUp")} title={t("modelRoutes.moveUp")} onClick={() => editMember(member.key, { tier: member.tier === 0 ? -1 : member.tier - 1 })}><ArrowUp /></Button>
                  <Button type="button" size="icon-sm" variant="ghost" disabled={tier === layers.length - 1 && layer.length === 1} aria-label={t("modelRoutes.moveDown")} title={t("modelRoutes.moveDown")} onClick={() => editMember(member.key, { tier: member.tier + 1 })}><ArrowDown /></Button>
                  <Button type="button" size="icon-sm" variant="ghost" aria-label={t("actions.delete")} title={t("actions.delete")} onClick={() => setMembers(draft.members.filter(other => other.key !== member.key))}><Trash2 /></Button>
                </span>
              </li>
            })}
          </ul>
        </div>) : <p className="text-sm text-muted-foreground">{t("modelRoutes.noMembers")}</p>}
        <div className="flex flex-col gap-2 rounded-lg border border-dashed p-2.5">
          <div className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
            {t("modelRoutes.addTo")}
            <ToggleGroup type="single" variant="outline" size="sm" value={String(Math.min(addTier, layers.length))} onValueChange={value => { if (value) setAddTier(Number(value)) }} aria-label={t("modelRoutes.addTo")}>
              {[...layers.keys(), layers.length].map(tier => <ToggleGroupItem key={tier} value={String(tier)}>{tier < layers.length ? t("modelRoutes.tierTitle", { n: tier + 1 }) : t("modelRoutes.newTier")}</ToggleGroupItem>)}
            </ToggleGroup>
          </div>
          <MemberAdder idPrefix={idPrefix} onAdd={(providerId, upstreamModel) => {
            if (draft.members.some(member => member.providerId === providerId && member.upstreamModel === upstreamModel)) { toast.error(t("modelRoutes.duplicateMember")); return }
            setMembers([...draft.members, { key: newKey(), providerId, upstreamModel, tier: Math.min(addTier, layers.length), weight: 1, enabled: true }])
          }} />
        </div>
      </div>
    </PageSection>

    {save.error ? <ErrorNotice error={save.error} /> : null}
    <div className="flex flex-wrap items-center gap-2">
      {route ? <ConfirmButton title={t("confirm.deleteTitle", { name: route.name })} onConfirm={() => remove.mutate()} disabled={remove.isPending}><Trash2 data-icon="inline-start" />{t("modelRoutes.deleteRoute")}</ConfirmButton> : null}
      <div className="ml-auto flex items-center gap-2">
        {route ? <Button type="button" variant="outline" disabled={!dirty || save.isPending} onClick={() => setDraft(null)}>{t("modelRoutes.discard")}</Button> : <Button type="button" variant="outline" onClick={onDone}>{t("actions.cancel")}</Button>}
        <Button type="button" disabled={(!dirty && !!route) || !draft.name.trim() || save.isPending} onClick={() => save.mutate()}>{t(route ? "actions.save" : "modelRoutes.create")}</Button>
      </div>
    </div>

  </div>
}
