import type { CredentialDto } from "@/generated/CredentialDto"
import type { CredentialQuotaCycleDto } from "@/generated/CredentialQuotaCycleDto"
import type { CredentialWriteRequest } from "@/generated/CredentialWriteRequest"
import { PlusIcon } from "lucide-react"
import { useTranslation } from "react-i18next"
import { CredentialCard } from "@/components/providers/credential-card"
import { CredentialDialog } from "@/components/providers/credential-dialog"
import { QueryState } from "@/components/query-state"
import { Button } from "@/components/ui/button"

type Props = {
  providerId: number
  credentials: Array<CredentialDto>
  cyclesByCredential: Map<number, Array<CredentialQuotaCycleDto>>
  credentialsLoading: boolean
  credentialsError: boolean
  cyclesLoading: boolean
  cyclesError: boolean
  savingCredentialId: number | null
  onSave: (value: CredentialWriteRequest, id?: number) => Promise<void>
}

export function CredentialList(props: Props) {
  const { t } = useTranslation()

  return (
    <section className="flex flex-col gap-3" aria-labelledby={`provider-${props.providerId}-credentials`}>
      <div className="flex flex-wrap items-center justify-between gap-2">
        <h3 id={`provider-${props.providerId}-credentials`} className="font-medium">{t("providers.credentials.title")}</h3>
        <CredentialDialog
          providerId={props.providerId}
          onSave={props.onSave}
          trigger={<Button variant="outline" size="sm"><PlusIcon data-icon="inline-start" />{t("providers.credentials.add")}</Button>}
        />
      </div>
      <QueryState
        loading={props.credentialsLoading}
        error={props.credentialsError ? t("providers.credentials.loadError") : ""}
        empty={!props.credentials.length ? t("providers.credentials.empty") : undefined}
      >
        <div className="flex flex-col gap-3">
          {props.credentials.map((credential) => (
            <CredentialCard
              key={credential.id}
              credential={credential}
              cycles={props.cyclesByCredential.get(credential.id) ?? []}
              cyclesLoading={props.cyclesLoading}
              cyclesError={props.cyclesError}
              saving={props.savingCredentialId === credential.id}
              onSave={props.onSave}
            />
          ))}
        </div>
      </QueryState>
    </section>
  )
}
