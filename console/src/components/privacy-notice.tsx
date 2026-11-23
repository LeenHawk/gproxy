import { useState, type ReactNode } from "react"
import { useTranslation } from "react-i18next"
import { quitApplication } from "@/api/preferences"
import { SetupFrame } from "@/pages/setup/frame"
import { Button } from "@/components/ui/button"
import { Dialog, DialogBody, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle, DialogTrigger } from "@/components/ui/dialog"
import english from "../../../distribution/mobile/privacy/privacy-en-US.txt?raw"
import chinese from "../../../distribution/mobile/privacy/privacy-zh-CN.txt?raw"
import russian from "../../../distribution/mobile/privacy/privacy-ru.txt?raw"

/** The notice's effective-date line: a new date asks for consent again. */
const VERSION = english.split("\n")[1]
const ACCEPTED_KEY = "gproxy.privacy.accepted"

function useNotice() {
  const { i18n } = useTranslation()
  return i18n.language.startsWith("zh") ? chinese : i18n.language === "ru" ? russian : english
}

export function PrivacyNotice({ link = false }: { link?: boolean }) {
  const { t } = useTranslation()
  const notice = useNotice()
  return <Dialog>
    <DialogTrigger asChild><Button type="button" variant={link ? "link" : "outline"} className={link ? "h-auto p-0" : undefined}>{t("shellPreferences.privacyNotice")}</Button></DialogTrigger>
    <DialogContent closeLabel={t("actions.close")} className="sm:max-w-2xl">
      <DialogHeader><DialogTitle>{t("shellPreferences.privacyNotice")}</DialogTitle></DialogHeader>
      <DialogBody>
        <DialogDescription className="whitespace-pre-wrap">{notice}</DialogDescription>
      </DialogBody>
      <DialogFooter closeLabel={t("actions.close")} />
    </DialogContent>
  </Dialog>
}

function accepted() {
  try { return window.localStorage.getItem(ACCEPTED_KEY) === VERSION } catch { return false }
}

/** The application's first screen until the current notice is accepted. */
export function PrivacyConsent({ children }: { children: ReactNode }) {
  const { t } = useTranslation()
  const notice = useNotice()
  const [agreed, setAgreed] = useState(accepted)
  const [quitting, setQuitting] = useState(false)
  if (agreed) return children
  const agree = () => {
    try { window.localStorage.setItem(ACCEPTED_KEY, VERSION) } catch { /* Asked again next launch. */ }
    setAgreed(true)
  }
  const decline = () => {
    setQuitting(true)
    quitApplication().catch(() => setQuitting(false))
  }
  return <SetupFrame current={0}>
    <section aria-labelledby="privacy-consent-title" className="flex flex-col gap-4">
      <h1 id="privacy-consent-title" className="text-xl font-semibold">{t("privacyConsent.title")}</h1>
      <p className="text-sm text-muted-foreground">{t("privacyConsent.intro")}</p>
      <div tabIndex={0} className="max-h-[55svh] overflow-y-auto rounded-md border p-4 text-sm whitespace-pre-wrap">{notice}</div>
      <div className="flex flex-col-reverse gap-2 sm:flex-row sm:justify-end">
        <Button type="button" variant="outline" disabled={quitting} onClick={decline}>{t("privacyConsent.decline")}</Button>
        <Button type="button" disabled={quitting} onClick={agree}>{t("privacyConsent.agree")}</Button>
      </div>
    </section>
  </SetupFrame>
}
