import { LanguagesIcon, MoonIcon, SunIcon } from "lucide-react"
import { useTranslation } from "react-i18next"
import { Button } from "@/components/ui/button"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { SUPPORTED_LANGS, setLanguage, type LangCode } from "@/i18n"
import { useTheme } from "@/lib/theme-context"
import type { Theme } from "@/lib/theme-state"

const themes: Array<Theme> = ["light", "dark", "system"]

export function LocaleControls({ showTheme = true }: { showTheme?: boolean }) {
  const { t, i18n } = useTranslation()
  const { theme, setTheme } = useTheme()
  return (
    <div className="flex items-center gap-1">
      {showTheme ? <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button variant="ghost" size="icon-sm" aria-label={t("common.theme.label")}>
            <SunIcon className="dark:hidden" aria-hidden />
            <MoonIcon className="hidden dark:block" aria-hidden />
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end" className="w-36">
          <DropdownMenuRadioGroup value={theme} onValueChange={(value) => setTheme(value as Theme)}>
            {themes.map((value) => (
              <DropdownMenuRadioItem key={value} value={value}>{t(`common.theme.${value}`)}</DropdownMenuRadioItem>
            ))}
          </DropdownMenuRadioGroup>
        </DropdownMenuContent>
      </DropdownMenu> : null}
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button variant="ghost" size="icon-sm" aria-label={t("common.language")}>
            <LanguagesIcon aria-hidden />
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end" className="w-40">
          <DropdownMenuGroup>
            <DropdownMenuRadioGroup value={i18n.language} onValueChange={(value) => void setLanguage(value as LangCode)}>
              {SUPPORTED_LANGS.map((language) => (
                <DropdownMenuRadioItem key={language} value={language}>{t(`common.languages.${language}`)}</DropdownMenuRadioItem>
              ))}
            </DropdownMenuRadioGroup>
          </DropdownMenuGroup>
        </DropdownMenuContent>
      </DropdownMenu>
    </div>
  )
}
