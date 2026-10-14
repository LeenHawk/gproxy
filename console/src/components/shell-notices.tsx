import { useQuery } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { SHELL_PREFERENCES_KEY, shellPreferences } from "@/api/preferences"
import { Alert, AlertDescription } from "@/components/ui/alert"

export function ShellNotices() {
  const { t } = useTranslation()
  const state = useQuery({ queryKey: SHELL_PREFERENCES_KEY, queryFn: shellPreferences, retry: false })
  const error = state.data?.trayError ?? state.data?.ohos?.trayError
  if (!error) return null
  return <Alert><AlertDescription>{t("shellPreferences.trayFailed", { error })}</AlertDescription></Alert>
}
