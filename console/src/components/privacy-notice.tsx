import { useTranslation } from "react-i18next"
import { Button } from "@/components/ui/button"
import { Dialog, DialogBody, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle, DialogTrigger } from "@/components/ui/dialog"
import english from "../../../distribution/mobile/privacy/privacy-en-US.txt?raw"
import chinese from "../../../distribution/mobile/privacy/privacy-zh-CN.txt?raw"

export function PrivacyNotice() {
  const { t, i18n } = useTranslation()
  return <Dialog>
    <DialogTrigger asChild><Button type="button" variant="outline">{t("shellPreferences.privacyNotice")}</Button></DialogTrigger>
    <DialogContent closeLabel={t("actions.close")} className="sm:max-w-2xl">
      <DialogHeader><DialogTitle>{t("shellPreferences.privacyNotice")}</DialogTitle></DialogHeader>
      <DialogBody>
        <DialogDescription className="whitespace-pre-wrap">{i18n.language.startsWith("zh") ? chinese : english}</DialogDescription>
      </DialogBody>
      <DialogFooter closeLabel={t("actions.close")} />
    </DialogContent>
  </Dialog>
}
