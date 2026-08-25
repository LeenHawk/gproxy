import { useTranslation } from "react-i18next"
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { SUPPORTED_LANGS, setLanguage, type LangCode } from "@/i18n"

export function LocaleSwitcher() {
  const { t, i18n } = useTranslation()
  return (
    <Select value={i18n.language} onValueChange={(value) => void setLanguage(value as LangCode)}>
      <SelectTrigger size="sm" aria-label={t("common.language")}>
        <SelectValue />
      </SelectTrigger>
      <SelectContent>
        <SelectGroup>
          {SUPPORTED_LANGS.map((language) => (
            <SelectItem key={language} value={language}>{t(`common.languages.${language}`)}</SelectItem>
          ))}
        </SelectGroup>
      </SelectContent>
    </Select>
  )
}
