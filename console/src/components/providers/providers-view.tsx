import type { ChannelDto } from "@/generated/ChannelDto"
import type { CredentialDto } from "@/generated/CredentialDto"
import type { CredentialQuotaCycleDto } from "@/generated/CredentialQuotaCycleDto"
import type { CredentialWriteRequest } from "@/generated/CredentialWriteRequest"
import type { ProviderDto } from "@/generated/ProviderDto"
import type { ProviderWriteRequest } from "@/generated/ProviderWriteRequest"
import type { TlsPresetDto } from "@/generated/TlsPresetDto"
import { PlusIcon } from "lucide-react"
import { useMemo } from "react"
import { useTranslation } from "react-i18next"
import { PageHeader } from "@/components/page-header"
import { ProviderCard } from "@/components/providers/provider-card"
import { ProviderDialog } from "@/components/providers/provider-dialog"
import { QueryState } from "@/components/query-state"
import { Button } from "@/components/ui/button"

type Props = {
  providers: Array<ProviderDto>
  providersLoading: boolean
  providersError: boolean
  channels: Array<ChannelDto>
  channelsLoading: boolean
  channelsError: boolean
  presets: Array<TlsPresetDto>
  presetsLoading: boolean
  presetsError: boolean
  credentials: Array<CredentialDto>
  credentialsLoading: boolean
  credentialsError: boolean
  cycles: Array<CredentialQuotaCycleDto>
  cyclesLoading: boolean
  cyclesError: boolean
  savingProviderId: number | null
  savingCredentialId: number | null
  onSaveProvider: (value: ProviderWriteRequest, id?: number) => Promise<void>
  onSaveCredential: (value: CredentialWriteRequest, id?: number) => Promise<void>
}

export function ProvidersView(props: Props) {
  const { t } = useTranslation()
  const credentialsByProvider = useMemo(() => {
    const groups = new Map<number, Array<CredentialDto>>()
    for (const credential of props.credentials) {
      const group = groups.get(credential.provider_id) ?? []
      group.push(credential)
      groups.set(credential.provider_id, group)
    }
    return groups
  }, [props.credentials])
  const cyclesByCredential = useMemo(() => {
    const groups = new Map<number, Array<CredentialQuotaCycleDto>>()
    for (const cycle of props.cycles) {
      const group = groups.get(cycle.credential_id) ?? []
      group.push(cycle)
      groups.set(cycle.credential_id, group)
    }
    return groups
  }, [props.cycles])

  return (
    <section className="flex flex-col gap-5">
      <PageHeader
        title={t("providers.title")}
        description={t("providers.subtitle")}
        actions={(
          <ProviderDialog
            channels={props.channels}
            channelsLoading={props.channelsLoading}
            channelsError={props.channelsError}
            presets={props.presets}
            presetsLoading={props.presetsLoading}
            presetsError={props.presetsError}
            onSave={props.onSaveProvider}
            trigger={<Button><PlusIcon data-icon="inline-start" />{t("providers.add")}</Button>}
          />
        )}
      />
      <QueryState
        loading={props.providersLoading}
        error={props.providersError ? t("providers.loadError") : ""}
        empty={!props.providers.length ? t("providers.empty") : undefined}
      >
        <div className="flex flex-col gap-4">
          {props.providers.map((provider) => (
            <ProviderCard
              key={provider.id}
              provider={provider}
              channels={props.channels}
              channelsLoading={props.channelsLoading}
              channelsError={props.channelsError}
              presets={props.presets}
              presetsLoading={props.presetsLoading}
              presetsError={props.presetsError}
              credentials={credentialsByProvider.get(provider.id) ?? []}
              cyclesByCredential={cyclesByCredential}
              credentialsLoading={props.credentialsLoading}
              credentialsError={props.credentialsError}
              cyclesLoading={props.cyclesLoading}
              cyclesError={props.cyclesError}
              savingProviderId={props.savingProviderId}
              savingCredentialId={props.savingCredentialId}
              onSaveProvider={props.onSaveProvider}
              onSaveCredential={props.onSaveCredential}
            />
          ))}
        </div>
      </QueryState>
    </section>
  )
}
