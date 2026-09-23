import { useState, type FormEvent } from "react"
import { useQuery } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { family } from "@/api/admin"
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
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"

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
  const change = (key: string, value: string | boolean) => setValues((current) => ({ ...current, [key]: value }))
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
  return <form onSubmit={submit} className="flex min-h-0 flex-col"><DialogBody>
    {parseError || error ? <ErrorNotice error={parseError ?? error} /> : null}
    <Tabs defaultValue="basic"><TabsList><TabsTrigger value="basic">{t("clientProfile.basic")}</TabsTrigger><TabsTrigger value="identity">{t("clientProfile.emulation")}</TabsTrigger><TabsTrigger value="transport">{t("clientProfile.transport")}</TabsTrigger></TabsList>
      <TabsContent value="basic"><FieldGroup>{text("name", "text", true)}{choice("backend")}</FieldGroup></TabsContent>
      <TabsContent value="identity"><FieldGroup className="sm:grid-cols-1"><Field><FieldLabel htmlFor="profile-identity">{t("clientProfile.emulation")}</FieldLabel><Select value={identity} disabled={pending} onValueChange={(next) => { if (next === "__custom") { const preset = presets.find((p) => p.id === identity); if (preset) setCustom(JSON.stringify(preset.emulation, null, 2)) } setIdentity(next) }}><SelectTrigger id="profile-identity"><SelectValue /></SelectTrigger><SelectContent><SelectGroup><SelectItem value="__none">{t("clientProfile.none")}</SelectItem>{presets.map((p) => <SelectItem key={p.id} value={p.id}>{p.label}</SelectItem>)}<SelectItem value="__custom">{t("clientProfile.custom")}</SelectItem></SelectGroup></SelectContent></Select></Field>{identity === "__custom" ? <Field><FieldLabel htmlFor="profile-custom">{t("clientProfile.custom")}</FieldLabel><Textarea id="profile-custom" rows={12} value={custom} disabled={pending} onChange={(e) => setCustom(e.target.value)} className="font-mono" /></Field> : null}</FieldGroup></TabsContent>
      <TabsContent value="transport"><FieldGroup>{choice("retry")}{numbers.map((key) => text(key, "number", true))}{decoders.map((key) => <Field key={key} orientation="horizontal"><FieldLabel htmlFor={`profile-${key}`}>{key}</FieldLabel><Switch id={`profile-${key}`} checked={Boolean(values[key])} disabled={pending} onCheckedChange={(v) => change(key, v)} /></Field>)}</FieldGroup></TabsContent>
    </Tabs>
  </DialogBody><DialogFooter><Button type="submit" disabled={pending || !String(values.name).trim()}>{t(original ? "actions.save" : "actions.create")}</Button></DialogFooter></form>
}
