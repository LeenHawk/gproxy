import { BookOpenIcon, ExternalLinkIcon, HeartIcon, CodeIcon } from "lucide-react"
import { useTranslation } from "react-i18next"
import { Page, PageHeader } from "@/components/page"
import { useQuery } from "@tanstack/react-query"
import { INFO_KEY, instanceInfo } from "@/api/settings"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardDescription, CardFooter, CardHeader, CardTitle } from "@/components/ui/card"
import { DOCS_URL, REPO_URL, SPONSORS_URL } from "@/lib/project-links"

export function AboutPage() {
  const { t } = useTranslation()
  const info = useQuery({ queryKey: INFO_KEY, queryFn: instanceInfo })
  return (
    <Page>
      <PageHeader title={t("about.title")} />
      <p className="text-sm text-muted-foreground">{t("about.subtitle")}</p>
      <div className="grid max-w-5xl items-start gap-6 xl:grid-cols-[minmax(0,2fr)_minmax(0,1fr)]">
        <Card>
          <CardHeader>
            <div className="mb-3 flex items-center gap-3">
              <img src={`${import.meta.env.BASE_URL}favicon-96x96.png`} width={48} height={48} className="size-12 rounded-lg" alt="" />
              <div className="flex flex-col gap-1">
                <CardTitle>GPROXY</CardTitle>
                {info.data ? <p className="text-xs text-muted-foreground">{t("about.version", { version: info.data.version })} · <code title={info.data.hash}>{info.data.hash.slice(0, 9)}</code></p> : null}
              </div>
            </div>
            <CardDescription>{t("about.description")}</CardDescription>
          </CardHeader>
          <CardContent className="flex flex-col gap-5">
            <ul className="flex list-disc flex-col gap-3 pl-5 text-sm leading-6">
              <li>{t("about.features.protocols")}</li>
              <li>{t("about.features.accounts")}</li>
              <li>{t("about.features.control")}</li>
            </ul>
            <p className="text-sm leading-6 text-muted-foreground">{t("about.openSource")}</p>
          </CardContent>
          <CardFooter className="flex-wrap gap-2">
            <Button asChild variant="outline">
              <a href={REPO_URL} target="_blank" rel="noopener noreferrer">
                <CodeIcon data-icon="inline-start" aria-hidden />{t("about.source")}
              </a>
            </Button>
            <Button asChild variant="outline">
              <a href={DOCS_URL} target="_blank" rel="noopener noreferrer">
                <BookOpenIcon data-icon="inline-start" aria-hidden />{t("about.documentation")}
              </a>
            </Button>
            <Button asChild variant="ghost">
              <a href={`${REPO_URL}/issues`} target="_blank" rel="noopener noreferrer">
                {t("about.feedback")}<ExternalLinkIcon data-icon="inline-end" aria-hidden />
              </a>
            </Button>
          </CardFooter>
        </Card>
        <Card>
          <CardHeader>
            <HeartIcon className="mb-3 size-6 text-primary" aria-hidden />
            <CardTitle>{t("about.sponsor.title")}</CardTitle>
            <CardDescription>{t("about.sponsor.description")}</CardDescription>
          </CardHeader>
          <CardContent className="flex flex-col gap-4">
            <div className="flex flex-col gap-2 rounded-lg border p-3 text-sm">
              <p className="font-medium">{t("about.sponsor.goal.title")}</p>
              <ul className="flex list-disc flex-col gap-1 pl-5 leading-6">
                <li>{t("about.sponsor.goal.google")}</li>
                <li>{t("about.sponsor.goal.apple")}</li>
              </ul>
              <p className="leading-6 text-muted-foreground">{t("about.sponsor.goal.why")}</p>
            </div>
            <p className="text-sm leading-6 text-muted-foreground">{t("about.sponsor.thanks")}</p>
            <Button asChild className="w-full">
              <a href={SPONSORS_URL} target="_blank" rel="noopener noreferrer">
                <HeartIcon data-icon="inline-start" aria-hidden />{t("about.sponsor.action")}
                <ExternalLinkIcon data-icon="inline-end" aria-hidden />
              </a>
            </Button>
          </CardContent>
        </Card>
      </div>
    </Page>
  )
}
