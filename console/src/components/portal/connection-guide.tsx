import { useMemo } from "react"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import type { PortalModelDto } from "@/generated/PortalModelDto"
import { Button } from "@/components/ui/button"
import { Card, CardAction, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Empty, EmptyDescription, EmptyHeader, EmptyTitle } from "@/components/ui/empty"
import { Field, FieldLabel } from "@/components/ui/field"
import { SearchableSelect } from "@/components/searchable-select"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { connectionSnippets, connectionSource } from "@/components/portal/connection-snippets"
import { copyText } from "@/lib/copy-text"
import { CopySnippet } from "@/components/portal/copy-snippet"

export function ConnectionGuide({
  origin,
  apiKey,
  models,
  selectedModel,
  onModelChange,
}: {
  origin: string
  apiKey: string
  models: Array<PortalModelDto>
  selectedModel: string | null
  onModelChange: (model: string) => void
}) {
  const { t } = useTranslation()
  const selected = models.find((model) => model.name === selectedModel)
  const target = connectionTarget(origin, selectedModel)
  const snippets = useMemo(() => {
    if (!selectedModel || !selected) return []
    const sources = new Set(selected.capabilities.map((capability) => capability.source))
    return connectionSnippets({
      origin: target.origin,
      model: target.model,
      key: apiKey,
      keyPlaceholder: t("portal.connect.keyPlaceholder"),
      prompt: t("portal.connect.prompt"),
    }).filter((snippet) => sources.has(connectionSource[snippet.method]))
  }, [apiKey, selected, selectedModel, t, target.model, target.origin])

  async function copyOrigin() {
    try {
      await copyText(target.origin)
      toast.success(t("portal.connect.copied"))
    } catch {
      toast.error(t("portal.connect.copyError"))
    }
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("portal.connect.title")}</CardTitle>
        <CardDescription>{t("portal.connect.description")}</CardDescription>
        <CardAction>
          <Button size="sm" variant="outline" onClick={() => void copyOrigin()}>
            {t("portal.connect.copyBaseUrl")}
          </Button>
        </CardAction>
      </CardHeader>
      <CardContent className="flex flex-col gap-5">
        <dl className="grid gap-1">
          <dt className="text-xs text-muted-foreground">{t("portal.connect.baseUrl")}</dt>
          <dd className="break-all font-mono text-sm">{target.origin}</dd>
        </dl>
        {selectedModel ? (
          <>
            <Field>
              <FieldLabel htmlFor="portal-model">{t("portal.connect.model")}</FieldLabel>
              <SearchableSelect id="portal-model" value={selectedModel} options={models.map((model) => ({ value: model.name, label: model.name }))} placeholder={t("common.none")} searchPlaceholder={t("common.search")} emptyLabel={t("common.none")} ariaLabel={t("portal.connect.model")} onChange={onModelChange} />
            </Field>
            {snippets.length > 0 ? <Tabs defaultValue={snippets[0].method}>
              <TabsList className="max-w-full overflow-x-auto overflow-y-hidden">
                {snippets.map((snippet) => (
                  <TabsTrigger key={snippet.method} value={snippet.method}>
                    {t(`portal.connect.methods.${snippet.method}`)}
                  </TabsTrigger>
                ))}
              </TabsList>
              {snippets.map((snippet) => (
                <TabsContent key={snippet.method} value={snippet.method} className="pt-3">
                  <CopySnippet
                    title={t(`portal.connect.methods.${snippet.method}`)}
                    display={snippet.display}
                    copyPayload={snippet.copy}
                  />
                </TabsContent>
              ))}
            </Tabs> : (
              <Empty>
                <EmptyHeader>
                  <EmptyTitle>{t("portal.connect.noCompatibleExamples")}</EmptyTitle>
                  <EmptyDescription>{t("portal.connect.noCompatibleExamplesDescription")}</EmptyDescription>
                </EmptyHeader>
              </Empty>
            )}
          </>
        ) : (
          <Empty>
            <EmptyHeader>
              <EmptyTitle>{t("portal.models.empty")}</EmptyTitle>
              <EmptyDescription>{t("portal.models.emptyDescription")}</EmptyDescription>
            </EmptyHeader>
          </Empty>
        )}
      </CardContent>
    </Card>
  )
}

function connectionTarget(origin: string, model: string | null) {
  const separator = model?.indexOf("/") ?? -1
  return model && separator > 0 && separator < model.length - 1
    ? { origin: `${origin}/${model.slice(0, separator)}`, model: model.slice(separator + 1) }
    : { origin, model: model ?? "" }
}
