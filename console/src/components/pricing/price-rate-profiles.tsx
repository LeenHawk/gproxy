import { useMemo, useState } from "react"
import { useQuery } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { defaultModelCatalog, priceCatalog } from "@/api/control"
import type { PriceProfileKindDto } from "@/generated/PriceProfileKindDto"
import type { PriceRateDto } from "@/generated/PriceRateDto"
import type { PriceRuleDto } from "@/generated/PriceRuleDto"
import { PriceRateGroup } from "@/components/pricing/price-rate-group"
import { SearchableSelect } from "@/components/searchable-select"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { Card, CardAction, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Field, FieldLabel } from "@/components/ui/field"
import { Skeleton } from "@/components/ui/skeleton"
import { PriceRateDialog } from "./price-rate-dialog"

const PROFILE_KINDS: Array<PriceProfileKindDto> = ["generation", "embedding", "rerank", "image", "audio", "video"]

export function PriceRateProfiles({ modelId, rule, rules, rates, deleting, onDelete }: {
  modelId: string
  rule: PriceRuleDto
  rules: Array<PriceRuleDto>
  rates: Array<PriceRateDto>
  deleting: boolean
  onDelete: (id: number) => void
}) {
  const { t } = useTranslation()
  const catalog = useQuery({ queryKey: ["price-catalog"], queryFn: priceCatalog })
  const models = useQuery({ queryKey: ["default-model-catalog"], queryFn: defaultModelCatalog })
  const model = useMemo(() => {
    const entries = models.data?.models ?? []
    const exact = entries.find((entry) => entry.model_id.toLowerCase() === modelId.toLowerCase())
    if (exact) return exact
    const basename = modelId.toLowerCase().split("/").at(-1)
    const matches = entries.filter((entry) => entry.model_id.toLowerCase().split("/").at(-1) === basename)
    return matches.length === 1 ? matches[0] : undefined
  }, [modelId, models.data])
  const initial = inferProfile(model?.output_modalities ?? [], rates)

  if (catalog.isError || models.isError) return <Alert variant="destructive"><AlertTitle>{t("pricing.catalog.loadErrorTitle")}</AlertTitle><AlertDescription>{t("pricing.catalog.loadError")}</AlertDescription></Alert>
  if (!catalog.data || !models.data) return <div className="flex flex-col gap-3"><Skeleton className="h-20" /><Skeleton className="h-56" /></div>
  return <ProfileContent key={`${rule.id}-${initial}`} initial={initial} profiles={catalog.data.profiles} rule={rule} rules={rules} rates={rates} deleting={deleting} onDelete={onDelete} />
}

function ProfileContent({ initial, profiles, rule, rules, rates, deleting, onDelete }: {
  initial: PriceProfileKindDto
  profiles: Awaited<ReturnType<typeof priceCatalog>>["profiles"]
  rule: PriceRuleDto
  rules: Array<PriceRuleDto>
  rates: Array<PriceRateDto>
  deleting: boolean
  onDelete: (id: number) => void
}) {
  const { t } = useTranslation()
  const [kind, setKind] = useState<PriceProfileKindDto>(initial)
  const profile = profiles.find((item) => item.kind === kind)
  const tools = profiles.find((item) => item.kind === "tools")
  const known = new Set(profiles.flatMap((item) => item.metrics.map((metric) => metric.metric)))
  const custom = rates.filter((rate) => !known.has(rate.metric))
  const options = PROFILE_KINDS.map((value) => ({ value, label: t(`pricing.profiles.${value}.title`), keywords: value }))
  return <div className="flex flex-col gap-4">
    <Card>
      <CardHeader>
        <CardTitle>{t("pricing.profiles.title")}</CardTitle>
        <CardDescription>{t("pricing.profiles.description")}</CardDescription>
        <CardAction><PriceRateDialog rules={rules} fixedRuleId={rule.id} trigger={<Button size="sm" variant="outline">{t("pricing.rates.addCustom")}</Button>} /></CardAction>
      </CardHeader>
      <CardContent>
        <Field>
          <FieldLabel htmlFor={`price-profile-${rule.id}`}>{t("pricing.profiles.modelType")}</FieldLabel>
          <SearchableSelect id={`price-profile-${rule.id}`} value={kind} options={options} placeholder={t("pricing.profiles.select")} searchPlaceholder={t("pricing.profiles.search")} emptyLabel={t("pricing.profiles.noMatches")} ariaLabel={t("pricing.profiles.modelType")} onChange={(value) => setKind(value as PriceProfileKindDto)} />
        </Field>
      </CardContent>
    </Card>
    {profile ? <PriceRateGroup profile={profile} rule={rule} rules={rules} rates={rates} deleting={deleting} onDelete={onDelete} /> : null}
    {tools ? <PriceRateGroup profile={tools} rule={rule} rules={rules} rates={rates} deleting={deleting} onDelete={onDelete} collapsible /> : null}
    {custom.length ? <PriceRateGroup title={t("pricing.profiles.custom.title")} description={t("pricing.profiles.custom.description")} customRates={custom} rule={rule} rules={rules} rates={rates} deleting={deleting} onDelete={onDelete} collapsible /> : null}
  </div>
}

function inferProfile(output: Array<string>, rates: Array<PriceRateDto>): PriceProfileKindDto {
  for (const [modality, kind] of [["embeddings", "embedding"], ["rerank", "rerank"], ["video", "video"], ["speech", "audio"], ["transcription", "audio"], ["audio", "audio"], ["image", "image"]] as const) {
    if (output.includes(modality)) return kind
  }
  const names = new Set(rates.map((rate) => rate.metric))
  if ([...names].some((name) => name.startsWith("video_"))) return "video"
  if (names.has("audio_seconds")) return "audio"
  if (names.has("search_units")) return "rerank"
  if (names.has("image_outputs")) return "image"
  return "generation"
}
