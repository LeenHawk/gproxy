import { useEffect, useRef, useState } from "react"
import { useTranslation } from "react-i18next"
import { ArrowRight, ChevronDown } from "lucide-react"
import { providerPath } from "@/api/configuration"
import { useNavigate } from "@/lib/router"
import { usePageTitle } from "@/lib/use-page-title"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardFooter, CardHeader, CardTitle } from "@/components/ui/card"
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible"
import { ProviderCredentials } from "@/pages/credentials"
import { SetupFrame } from "./frame"
import { setupSteps, type PendingCredentials } from "./pending"

/** The wizard's last step: a credential for each provider it made, every one skippable. */
export function SetupCredentials({ pending }: { pending: PendingCredentials }) {
  const { t } = useTranslation()
  const navigate = useNavigate()
  const [open, setOpen] = useState<string | null>(pending.providers[0]?.id ?? null)
  const mainRef = useRef<HTMLElement>(null)
  useEffect(() => { mainRef.current?.querySelector<HTMLElement>("h1")?.focus() }, [])
  usePageTitle(t("setup.credentials"))
  const finish = () => {
    pending.done()
    navigate(pending.providers.length === 1 ? providerPath(pending.providers[0].id) : "/providers")
  }
  return <SetupFrame mainRef={mainRef} steps={setupSteps(t)} current={3}>
    <Card>
      <CardHeader><CardTitle headingLevel={1} tabIndex={-1}>{t("setup.credentials")}</CardTitle></CardHeader>
      <CardContent className="flex flex-col gap-4">
        <p className="text-sm text-muted-foreground">{t("setup.credentialsHint")}</p>
        {pending.providers.map(provider => <Collapsible key={provider.id} open={open === provider.id} onOpenChange={value => setOpen(value ? provider.id : null)} className="rounded-lg border">
          <CollapsibleTrigger asChild><Button variant="ghost" className="h-auto w-full justify-between px-4 py-3"><span className="flex min-w-0 flex-col items-start gap-0.5"><span className="truncate">{provider.displayName ?? provider.name}</span><span className="truncate font-mono text-xs text-muted-foreground">/{provider.name}</span></span><ChevronDown className={open === provider.id ? "rotate-180" : undefined} /></Button></CollapsibleTrigger>
          <CollapsibleContent className="border-t p-4"><ProviderCredentials providerId={provider.id} /></CollapsibleContent>
        </Collapsible>)}
      </CardContent>
      <CardFooter className="justify-end"><Button onClick={finish}>{t("setup.openConsole")}<ArrowRight data-icon="inline-end" /></Button></CardFooter>
    </Card>
  </SetupFrame>
}
