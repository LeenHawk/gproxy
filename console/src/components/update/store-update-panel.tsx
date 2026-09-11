import { useTranslation } from "react-i18next"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardDescription, CardFooter, CardHeader, CardTitle } from "@/components/ui/card"
import { buildIdentity } from "@/lib/build-info"

export function StoreUpdatePanel() {
  const { t } = useTranslation()
  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("update.store.title")}</CardTitle>
        <CardDescription>{t("update.store.description")}</CardDescription>
      </CardHeader>
      <CardContent className="flex flex-col gap-3">
        <div className="flex items-center justify-between gap-3">
          <span className="text-sm text-muted-foreground">{t("update.check.current")}</span>
          <Badge variant="outline" className="font-mono">{buildIdentity().version}</Badge>
        </div>
        <p className="text-sm text-muted-foreground">{t("update.store.hint")}</p>
      </CardContent>
      <CardFooter>
        <Button asChild><a href="ms-windows-store://downloadsandupdates">{t("update.store.open")}</a></Button>
      </CardFooter>
    </Card>
  )
}
