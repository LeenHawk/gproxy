import { useEffect, useState } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import { SHELL_PREFERENCES_KEY, shellPreferences, saveShellPreferences, type ShellPreferencesStatus } from "@/api/preferences"
import { ShellPreferenceFields } from "@/components/shell-preference-fields"
import { ErrorNotice, QueryState } from "@/components/state"
import { Button } from "@/components/ui/button"
import { OhosSettings } from "./ohos"
import { AndroidPermissions } from "./mobile-permissions"
import { FieldGroup } from "@/components/ui/field"
import { Alert, AlertDescription } from "@/components/ui/alert"
import { FontSettings } from "@/components/font-settings"
import { PrivacyNotice } from "@/components/privacy-notice"

export function ApplicationSettings() {
  const client = useQueryClient()
  useEffect(() => {
    const refresh = () => { void client.invalidateQueries({ queryKey: SHELL_PREFERENCES_KEY }) }
    window.addEventListener("focus", refresh)
    return () => window.removeEventListener("focus", refresh)
  }, [client])
  const data = useQuery({ queryKey: SHELL_PREFERENCES_KEY, queryFn: shellPreferences, refetchOnWindowFocus: true })
  return <div className="flex flex-col gap-6">
    <QueryState isPending={data.isPending} error={data.error}>
      {data.data?.ohos ? <OhosSettings original={data.data.ohos} /> : data.data ? <ApplicationForm key={JSON.stringify(data.data)} original={data.data} /> : null}
    </QueryState>
    <FontSettings />
    <div><PrivacyNotice /></div>
  </div>
}

function ApplicationForm({ original }: { original: ShellPreferencesStatus }) {
  const { t, i18n } = useTranslation()
  const client = useQueryClient()
  const [draft, setDraft] = useState(original)
  const saved = useMutation({ mutationFn: saveShellPreferences,
    onSuccess: value => { client.setQueryData(SHELL_PREFERENCES_KEY, value); toast.success(t("toast.saved")) },
  })
  return <form className="flex flex-col gap-6" onSubmit={event => { event.preventDefault(); saved.mutate({ ...draft, language: i18n.language }) }}>
    {original.startupError ? <ErrorNotice error={new Error(original.startupError)} /> : null}
    {original.trayError ? <Alert><AlertDescription>{t("shellPreferences.trayFailed", { error: original.trayError })}</AlertDescription></Alert> : null}
    <FieldGroup className="grid gap-6 sm:grid-cols-2">
      <ShellPreferenceFields value={draft} desktop={original.desktop} canAutoStart={original.canAutoStart} disabled={saved.isPending}
        onChange={value => setDraft({ ...draft, ...value })} />
    </FieldGroup>
    {original.platform === "android" ? <AndroidPermissions /> : null}
    {saved.error ? <ErrorNotice error={saved.error} /> : null}
    <div className="flex justify-end"><Button type="submit" disabled={saved.isPending || (!original.desktop && !original.canAutoStart)}>{t("actions.save")}</Button></div>
  </form>
}
