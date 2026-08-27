import { useTranslation } from "react-i18next"
import { ConnectivityTest } from "@/components/connectivity-test"
import { Field, FieldDescription, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { InputGroup, InputGroupAddon, InputGroupInput } from "@/components/ui/input-group"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"

type Props = {
  id: string
  label: string
  strategy: string
  proxyUrl: string
  onLabel: (value: string) => void
  onStrategy: (value: string) => void
  onProxy: (value: string) => void
}

export function ProviderIdentityFields(props: Props) {
  const { t } = useTranslation()
  return (
    <>
      <Field>
        <FieldLabel htmlFor={`${props.id}-label`}>{t("providers.fields.label")}</FieldLabel>
        <Input id={`${props.id}-label`} value={props.label} onChange={(event) => props.onLabel(event.target.value)} />
        <FieldDescription>{t("providers.form.labelHint")}</FieldDescription>
      </Field>
      <div className="grid gap-4 sm:grid-cols-2">
        <Field>
          <FieldLabel htmlFor={`${props.id}-strategy`}>{t("providers.fields.credentialStrategy")}</FieldLabel>
          <Select value={props.strategy} onValueChange={props.onStrategy}>
            <SelectTrigger id={`${props.id}-strategy`} className="w-full"><SelectValue /></SelectTrigger>
            <SelectContent>
              <SelectItem value="round_robin">{t("providers.strategies.round_robin")}</SelectItem>
              <SelectItem value="sticky">{t("providers.strategies.sticky")}</SelectItem>
            </SelectContent>
          </Select>
          <FieldDescription>{t("providers.form.credentialStrategyHint")}</FieldDescription>
        </Field>
        <Field>
          <FieldLabel htmlFor={`${props.id}-proxy`}>{t("providers.fields.proxy")}</FieldLabel>
          <InputGroup>
            <InputGroupInput id={`${props.id}-proxy`} type="url" className="font-mono" value={props.proxyUrl} onChange={(event) => props.onProxy(event.target.value)} />
            <InputGroupAddon align="inline-end"><ConnectivityTest request={{ scope: "proxy", provider_id: null, credential_id: null, proxy_url: props.proxyUrl }} label={t("providers.fields.proxy")} disabled={!props.proxyUrl.trim()} /></InputGroupAddon>
          </InputGroup>
          <FieldDescription>{t("providers.form.proxyHint")}</FieldDescription>
        </Field>
      </div>
    </>
  )
}
