import { useTranslation } from "react-i18next"
import { type ProviderFormSource, useProviderForm } from "@/components/providers/provider-form"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"

export function ProviderSettingsPanel(source: ProviderFormSource) {
  const { t } = useTranslation()
  const form = useProviderForm(source)
  return (
    <Card size="sm">
      <CardHeader>
        <CardTitle>{t("providers.tabs.settings")}</CardTitle>
        <CardDescription>{t("providers.settings.description")}</CardDescription>
      </CardHeader>
      <CardContent>
        <form className="flex flex-col gap-6" onSubmit={form.submit}>
          {form.fields}
          {form.submitError}
          <div className="flex justify-end gap-2">
            <Button type="button" variant="outline" onClick={form.reset}>{t("common.actions.reset")}</Button>
            <Button type="submit" disabled={form.saving}>{t(form.saving ? "common.actions.saving" : "common.actions.save")}</Button>
          </div>
        </form>
      </CardContent>
    </Card>
  )
}
