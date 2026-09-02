import { useMemo } from "react"
import { useTranslation } from "react-i18next"
import { connectionSnippets } from "@/components/portal/connection-snippets"
import { CopyButton } from "@/components/public/copy-button"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"

export function ConnectPanel() {
  const { t } = useTranslation()
  const origin = window.location.origin
  const keyPlaceholder = t("portal.connect.keyPlaceholder")
  const model = t("public.connect.modelPlaceholder")
  const prompt = t("portal.connect.prompt")
  const snippets = useMemo(
    () => connectionSnippets({ origin, model, key: keyPlaceholder, keyPlaceholder, prompt }),
    [keyPlaceholder, model, origin, prompt],
  )

  return (
    <section id="connect" className="public-connect" aria-labelledby="public-connect-title">
      <div>
        <h2 id="public-connect-title" className="public-section-title">{t("public.connect.title")}</h2>
        <p className="public-section-lede">{t("public.connect.description")}</p>
        <div className="public-origin">
          <div>
            <span className="public-origin-label">{t("portal.connect.baseUrl")}</span>
            <code className="public-machine">{origin}</code>
          </div>
          <CopyButton value={origin} />
        </div>
        <p className="public-connect-note">
          {t("public.connect.note")} <a href="/portal">{t("public.connect.portalLink")}</a>
        </p>
      </div>
      <Tabs defaultValue={snippets[0].method} className="public-snippet">
        <div className="public-snippet-head">
          <TabsList variant="line" aria-label={t("public.connect.methodsLabel")}>
            {snippets.map((snippet) => (
              <TabsTrigger key={snippet.method} value={snippet.method}>{t(`portal.connect.methods.${snippet.method}`)}</TabsTrigger>
            ))}
          </TabsList>
        </div>
        {snippets.map((snippet) => (
          <TabsContent key={snippet.method} value={snippet.method} className="public-snippet-body">
            <pre className="public-machine"><code>{snippet.display}</code></pre>
            <CopyButton className="public-snippet-copy" value={snippet.copy} />
          </TabsContent>
        ))}
      </Tabs>
    </section>
  )
}
