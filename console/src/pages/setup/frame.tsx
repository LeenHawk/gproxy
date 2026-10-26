import type { ReactNode, RefObject } from "react"
import { useTranslation } from "react-i18next"
import { SUPPORTED_LANGS, setLanguage, type LangCode } from "@/i18n"
import { Badge } from "@/components/ui/badge"
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"

/** The wizard's page: the brand, the language, and where in the steps this is. */
export function SetupFrame({ steps, current, mainRef, children }: { steps?: string[]; current: number; mainRef?: RefObject<HTMLElement | null>; children: ReactNode }) {
  const { t, i18n } = useTranslation()
  return <main ref={mainRef} className="mx-auto flex min-h-svh w-full max-w-2xl flex-col justify-center gap-6 px-4 py-8 sm:px-6 sm:py-12">
    <header className="flex items-center justify-between gap-4">
      <div className="flex min-w-0 items-center gap-2"><img src={`${import.meta.env.BASE_URL}favicon-96x96.png`} alt="" className="size-10" /><span className="font-semibold">GPROXY</span></div>
      <Select value={i18n.language} onValueChange={value => void setLanguage(value as LangCode)}><SelectTrigger aria-label={t("setup.language")} className="w-auto"><SelectValue /></SelectTrigger><SelectContent><SelectGroup>{SUPPORTED_LANGS.map(code => <SelectItem key={code} value={code}>{t(`language.${code}`)}</SelectItem>)}</SelectGroup></SelectContent></Select>
    </header>
    {steps ? <ol role="list" className="flex flex-wrap justify-between gap-2" aria-label={t("setup.steps")}>{steps.map((title, index) => <li key={title} className="flex min-w-0 items-center gap-2 text-sm" aria-current={current === index ? "step" : undefined}><Badge variant={current === index ? "default" : "secondary"}>{index + 1}</Badge><span className="whitespace-normal wrap-anywhere">{title}</span></li>)}</ol> : null}
    {children}
  </main>
}
