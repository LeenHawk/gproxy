import { useState, type Dispatch, type SetStateAction } from "react"
import { useTranslation } from "react-i18next"
import { Search } from "lucide-react"
import type { ChannelDescriptor } from "@/generated/sdk"
import { Badge } from "@/components/ui/badge"
import { Checkbox } from "@/components/ui/checkbox"
import { InputGroup, InputGroupAddon, InputGroupInput } from "@/components/ui/input-group"

/** Every channel this build carries, to tick the ones a provider should be made for. */
export function ChannelPicker({ channels, selected, onChange, disabled }: { channels: ChannelDescriptor[]; selected: string[]; onChange: Dispatch<SetStateAction<string[]>>; disabled?: boolean }) {
  const { t } = useTranslation()
  const [search, setSearch] = useState("")
  const query = search.trim().toLowerCase()
  const visible = channels.filter(row => !query || row.displayName.toLowerCase().includes(query) || row.id.toLowerCase().includes(query))
  const toggle = (id: string, checked: boolean) => onChange(current => checked ? [...current, id] : current.filter(value => value !== id))
  return <div className="flex flex-col gap-3">
    <InputGroup><InputGroupAddon><Search /></InputGroupAddon><InputGroupInput value={search} onChange={event => setSearch(event.target.value)} aria-label={t("setup.searchChannels")} placeholder={t("setup.searchChannels")} /></InputGroup>
    {visible.length ? <ul className="grid max-h-[45vh] gap-2 overflow-y-auto sm:grid-cols-2">{visible.map(row => <li key={row.id}>
      <label className="flex min-h-11 cursor-pointer items-start gap-3 rounded-lg border px-3 py-2.5 text-sm has-data-checked:border-primary has-data-checked:bg-accent">
        <Checkbox className="mt-0.5" checked={selected.includes(row.id)} onCheckedChange={value => toggle(row.id, value === true)} disabled={disabled} />
        <span className="flex min-w-0 flex-col gap-1.5"><span className="wrap-anywhere">{row.displayName}</span><span className="flex flex-wrap gap-1">{row.credentialKinds.map(kind => <Badge key={kind} variant="secondary">{t(`credentialKinds.${kind}`, { defaultValue: kind })}</Badge>)}</span></span>
      </label>
    </li>)}</ul> : <p className="text-sm text-muted-foreground">{t("setup.noChannels")}</p>}
  </div>
}
