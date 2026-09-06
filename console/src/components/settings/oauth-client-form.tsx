import { useId, useState } from "react"
import { useTranslation } from "react-i18next"
import type { OAuthClientDto } from "@/generated/OAuthClientDto"
import type { OAuthClientWriteRequest } from "@/generated/OAuthClientWriteRequest"
import { Button } from "@/components/ui/button"
import { Field, FieldDescription, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Switch } from "@/components/ui/switch"

export function OAuthClientForm({ client, pending, onSave, onCancel }: {
  client?: OAuthClientDto
  pending: boolean
  onSave: (value: OAuthClientWriteRequest) => void
  onCancel: () => void
}) {
  const { t } = useTranslation()
  const id = useId()
  const [draft, setDraft] = useState<OAuthClientWriteRequest>(() => ({
    client_id: client?.client_id ?? "", name: client?.name ?? "",
    redirect_uris: client?.redirect_uris ?? [], enabled: client?.enabled ?? false,
  }))
  return (
    <form className="flex flex-col gap-4" onSubmit={(event) => { event.preventDefault(); onSave(draft) }}>
      <FieldGroup>
        <Field><FieldLabel htmlFor={`${id}-client`}>{t("settings.oauth.clientId")}</FieldLabel><Input id={`${id}-client`} required maxLength={128} pattern="[a-zA-Z0-9_.\-]+" disabled={client != null || pending} value={draft.client_id} onChange={(event) => setDraft({ ...draft, client_id: event.target.value })} /></Field>
        <Field><FieldLabel htmlFor={`${id}-name`}>{t("settings.oauth.name")}</FieldLabel><Input id={`${id}-name`} required maxLength={128} disabled={pending} value={draft.name} onChange={(event) => setDraft({ ...draft, name: event.target.value })} /></Field>
        <Field data-field-span="full">
          <FieldLabel>{t("settings.oauth.redirects")}</FieldLabel>
          <FieldDescription>{t("settings.oauth.redirectHint")}</FieldDescription>
          {draft.redirect_uris.map((uri, index) => (
            <div key={index} className="flex gap-2">
              <Input aria-label={t("settings.oauth.redirectNumber", { number: index + 1 })} type="url" required maxLength={2048} disabled={pending} value={uri} onChange={(event) => setDraft({ ...draft, redirect_uris: draft.redirect_uris.map((value, position) => position === index ? event.target.value : value) })} />
              <Button type="button" variant="outline" disabled={pending} onClick={() => setDraft({ ...draft, redirect_uris: draft.redirect_uris.filter((_, position) => position !== index) })}>{t("common.actions.delete")}</Button>
            </div>
          ))}
          <Button type="button" variant="outline" disabled={pending || draft.redirect_uris.length >= 32} onClick={() => setDraft({ ...draft, redirect_uris: [...draft.redirect_uris, ""] })}>{t("settings.oauth.addRedirect")}</Button>
        </Field>
        <Field orientation="horizontal"><FieldLabel htmlFor={`${id}-enabled`}>{t("common.status.enabled")}</FieldLabel><Switch id={`${id}-enabled`} disabled={pending} checked={draft.enabled} onCheckedChange={(enabled) => setDraft({ ...draft, enabled })} /></Field>
      </FieldGroup>
      <div className="flex gap-2"><Button type="submit" disabled={pending}>{t(pending ? "common.actions.saving" : "common.actions.save")}</Button><Button type="button" variant="outline" disabled={pending} onClick={onCancel}>{t("common.actions.cancel")}</Button></div>
    </form>
  )
}
