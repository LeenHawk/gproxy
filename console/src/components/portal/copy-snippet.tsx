import { CopyIcon } from "lucide-react"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import { Button } from "@/components/ui/button"
import { Card, CardAction, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import { copyText } from "@/components/portal/copy-text"

export function CopySnippet({ title, display, copyPayload }: { title: string; display: string; copyPayload: string }) {
  const { t } = useTranslation()

  async function copy() {
    try {
      await copyText(copyPayload)
      toast.success(t("portal.connect.copied"))
    } catch {
      toast.error(t("portal.connect.copyError"))
    }
  }

  return (
    <Card size="sm">
      <CardHeader>
        <CardTitle headingLevel={3}>{title}</CardTitle>
        <CardAction>
          <Button size="sm" variant="outline" onClick={() => void copy()}>
            <CopyIcon data-icon="inline-start" />
            {t("portal.connect.copy")}
          </Button>
        </CardAction>
      </CardHeader>
      <CardContent>
        <pre className="max-h-96 overflow-auto rounded-lg bg-muted p-3 text-xs leading-5">
          <code className="font-mono">{display}</code>
        </pre>
      </CardContent>
    </Card>
  )
}
