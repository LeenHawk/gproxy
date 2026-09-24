import { useId, useState, type FormEvent } from "react"
import { ChevronDown, ChevronsUpDown } from "lucide-react"
import { useQuery } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { configFamily as family } from "@/api/config-family"
import { api } from "@/api/client"
import type { ConnectionProfileDto, ConnectionProfileWrite, ConnectionProfilePatch, TlsPresetDto } from "@/generated/sdk"
import { CollectionPage } from "@/pages/identity/collection"
import { QueryState, ErrorNotice } from "@/components/state"
import { Button } from "@/components/ui/button"
import { Dialog, DialogBody, DialogContent, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { Field, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Textarea } from "@/components/ui/textarea"
import { Switch } from "@/components/ui/switch"
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Command, CommandInput, CommandList, CommandEmpty, CommandGroup, CommandItem } from "@/components/ui/command"
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover"
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible"

const profiles = family<ConnectionProfileDto, Partial<ConnectionProfileWrite>, Partial<ConnectionProfilePatch>>("/connection-profiles")
const options: Record<string, string[]> = { backend: ["reqwest", "reqwest_native", "wreq"], retry: ["never", "default"] }
const numbers = ["redirectMaxHops", "connectTimeoutMs", "poolIdleTimeoutMs", "poolMaxIdlePerHost"] as const
const decoders = ["gzip", "brotli", "deflate", "zstd"] as const

export function ClientsPage() {
  const { t } = useTranslation()
  const presets = useQuery({ queryKey: ["tls-presets"], queryFn: () => api<TlsPresetDto[]>("/admin/api/tls-presets") })
  return <QueryState isPending={presets.isPending} error={presets.error}>
    <CollectionPage id="connection-profiles" family={profiles} searchable rowId={(r) => r.id} rowLabel={(r) => r.name} fields={[]}
      columns={[{ key: "name", cell: (r) => r.name }, { key: "backend", cell: (r) => t(`clientProfile.${r.backend}`) }, { key: "emulation", cell: (r) => presets.data?.find((p) => JSON.stringify(p.emulation) === JSON.stringify(r.emulation))?.label ?? (r.emulation ? t("clientProfile.custom") : "—") }]}
      renderForm={(props) => <ProfileDialog {...props} presets={presets.data ?? []} />}
    />
  </QueryState>
}
type Props = { open: boolean; onOpenChange: (open: boolean) => void; original?: ConnectionProfileDto; onSubmit: (body: Record<string, unknown>) => void; pending: boolean; error: unknown; presets: TlsPresetDto[] }
function ProfileDialog(props: Props) {
  const { t } = useTranslation()
  return <Dialog open={props.open} onOpenChange={(v) => { if (!props.pending) props.onOpenChange(v) }}><DialogContent className="sm:max-w-2xl" aria-describedby={undefined} closeLabel={t("actions.close")}><DialogHeader><DialogTitle>{t(props.original ? "edit.connection-profiles" : "create.connection-profiles")}</DialogTitle></DialogHeader>{props.open ? <ProfileForm key={props.original?.id ?? "new"} {...props} /> : null}</DialogContent></Dialog>
}
function ProfileForm({ original, presets, onSubmit, pending, error }: Props) {
  const { t } = useTranslation()
  const initial: Record<string, string | boolean> = { name: "", backend: "reqwest", retry: "never", gzip: false, brotli: false, deflate: false, zstd: false, redirectMaxHops: "0", connectTimeoutMs: "10000", poolIdleTimeoutMs: "90000", poolMaxIdlePerHost: "32" }
  const [values, setValues] = useState(() => Object.fromEntries(Object.entries(initial).map(([key, fallback]) => [key, original ? (original[key as keyof ConnectionProfileDto] == null ? fallback : typeof fallback === "boolean" ? Boolean(original[key as keyof ConnectionProfileDto]) : String(original[key as keyof ConnectionProfileDto])) : fallback])))
  const [identity, setIdentity] = useState(() => original?.emulation ? presets.find((p) => JSON.stringify(p.emulation) === JSON.stringify(original.emulation))?.id ?? "__custom" : "__none")
  const [custom, setCustom] = useState(JSON.stringify(original?.emulation ?? { kind: "custom", headers: [] }, null, 2))
  const [parseError, setParseError] = useState<Error | null>(null)
  const change = (key: string, value: string | boolean) => {
    if (key === "backend" && value !== "wreq" && identity.startsWith("wreq:")) setIdentity("__none")
    setValues((current) => ({ ...current, [key]: value }))
  }
  const submit = (e: FormEvent) => {
    e.preventDefault()
    try {
      const emulation = identity === "__none" ? null : identity === "__custom" ? JSON.parse(custom) as unknown : presets.find((p) => p.id === identity)!.emulation
      const body: Record<string, unknown> = { ...values, name: String(values.name).trim(), emulation }
      for (const key of numbers) body[key] = Number(values[key])
      if (original) for (const key of Object.keys(body)) if (JSON.stringify(body[key]) === JSON.stringify(original[key as keyof ConnectionProfileDto])) delete body[key]
      setParseError(null); onSubmit(body)
    } catch { setParseError(new Error(t("form.invalidJson"))) }
  }
  const text = (key: string, type = "text", required = false) => <Field key={key}><FieldLabel htmlFor={`profile-${key}`}>{t(`clientProfile.${key}`)}</FieldLabel><Input id={`profile-${key}`} type={type} value={String(values[key])} disabled={pending} required={required} min={type === "number" ? 0 : undefined} step={type === "number" ? 1 : undefined} onChange={(e) => change(key, e.target.value)} /></Field>
  const choice = (key: string) => <Field key={key}><FieldLabel htmlFor={`profile-${key}`}>{t(`clientProfile.${key}`)}</FieldLabel><Select value={String(values[key])} disabled={pending} onValueChange={(v) => change(key, v)}><SelectTrigger id={`profile-${key}`}><SelectValue /></SelectTrigger><SelectContent><SelectGroup>{options[key].map((v) => <SelectItem key={v} value={v}>{t(`clientProfile.${v}`)}</SelectItem>)}</SelectGroup></SelectContent></Select></Field>
  return <form onSubmit={submit} className="flex min-h-0 flex-col"><DialogBody className="flex flex-col gap-5">
    {parseError || error ? <ErrorNotice error={parseError ?? error} /> : null}
    <FieldGroup>{text("name", "text", true)}{choice("backend")}</FieldGroup>
      <FieldGroup className="sm:grid-cols-1"><Field><FieldLabel className="sr-only" htmlFor="profile-identity">{t("clientProfile.emulation")}</FieldLabel><EmulationPicker value={identity} disabled={pending} presets={presets} onChange={(next) => { if (next === "__custom") { const preset = presets.find((p) => p.id === identity); if (preset) setCustom(JSON.stringify(preset.emulation, null, 2)) } if (next.startsWith("wreq:")) change("backend", "wreq"); setIdentity(next) }} /></Field>{identity === "__custom" ? <Field><FieldLabel htmlFor="profile-custom">{t("clientProfile.custom")}</FieldLabel><Textarea id="profile-custom" rows={12} value={custom} disabled={pending} onChange={(e) => setCustom(e.target.value)} className="font-mono" /></Field> : null}</FieldGroup>
    <Collapsible><CollapsibleTrigger asChild><Button type="button" variant="outline" className="group w-full justify-between">{t("clientProfile.advanced")}<ChevronDown className="transition-transform group-data-[state=open]:rotate-180" aria-hidden="true" /></Button></CollapsibleTrigger><CollapsibleContent className="pt-4"><FieldGroup>{choice("retry")}{numbers.map((key) => text(key, "number", true))}{decoders.map((key) => <Field key={key} orientation="horizontal"><FieldLabel htmlFor={`profile-${key}`}>{key}</FieldLabel><Switch id={`profile-${key}`} checked={Boolean(values[key])} disabled={pending} onCheckedChange={(v) => change(key, v)} /></Field>)}</FieldGroup></CollapsibleContent></Collapsible>
  </DialogBody><DialogFooter><Button type="submit" disabled={pending || !String(values.name).trim()}>{t(original ? "actions.save" : "actions.create")}</Button></DialogFooter></form>
}

function EmulationPicker({ value, onChange, presets, disabled }: { value: string; onChange: (value: string) => void; presets: TlsPresetDto[]; disabled: boolean }) {
  const { t } = useTranslation()
  const [open, setOpen] = useState(false)
  const listId = useId()
  const choices = [{ id: "__none", label: t("clientProfile.none") }, ...presets, { id: "__custom", label: t("clientProfile.custom") }]
  return <Popover modal open={open} onOpenChange={setOpen}>
    <PopoverTrigger asChild><Button id="profile-identity" type="button" role="combobox" aria-expanded={open} aria-controls={listId} aria-label={t("clientProfile.emulation")} disabled={disabled} variant="outline" className="w-full min-w-0 justify-between"><span className="truncate">{choices.find(c => c.id === value)?.label}</span><ChevronsUpDown aria-hidden="true" /></Button></PopoverTrigger>
    <PopoverContent align="start" className="w-[var(--radix-popover-trigger-width)] p-0">
      <Command><CommandInput placeholder={t("clientProfile.searchPresets")} aria-label={t("clientProfile.searchPresets")} /><CommandList id={listId}><CommandEmpty>{t("clientProfile.noPresets")}</CommandEmpty><CommandGroup>
        {choices.map(choice => <CommandItem key={choice.id} value={choice.id} keywords={[choice.label]} data-checked={choice.id === value} onSelect={() => { onChange(choice.id); setOpen(false) }}>{choice.label}</CommandItem>)}
      </CommandGroup></CommandList></Command>
    </PopoverContent>
  </Popover>
}
