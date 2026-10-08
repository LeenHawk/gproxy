import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { FONTS_KEY, downloadFonts, fontStatus, removeFonts, type FontStatus } from "@/api/fonts"
import { loadFonts } from "@/lib/fonts"
import { ErrorNotice } from "@/components/state"
import { Button } from "@/components/ui/button"
import { Card, CardHeader, CardTitle, CardDescription, CardContent, CardFooter } from "@/components/ui/card"
import { Progress } from "@/components/ui/progress"

export function FontSettings() {
  const { t } = useTranslation()
  const client = useQueryClient()
  const onSuccess = (value: FontStatus) => {
    client.setQueryData(FONTS_KEY, value)
    loadFonts()
  }
  const download = useMutation({ mutationFn: downloadFonts, onSuccess,
    onSettled: () => client.invalidateQueries({ queryKey: FONTS_KEY }) })
  const remove = useMutation({ mutationFn: removeFonts, onSuccess })
  const status = useQuery({ queryKey: FONTS_KEY, queryFn: fontStatus,
    refetchInterval: query => download.isPending || query.state.data?.downloading ? 500 : false })
  const busy = download.isPending || remove.isPending || status.data?.downloading
  const error = download.error ?? remove.error ?? status.error
  const progress = status.data?.total ? Math.round(status.data.completed * 100 / status.data.total) : 0
  return <Card size="sm">
    <CardHeader>
      <CardTitle>{t("fonts.title")}</CardTitle>
      <CardDescription>{t("fonts.description")}</CardDescription>
    </CardHeader>
    <CardContent className="flex flex-col gap-3">
      <p role="status" className="text-sm">{download.isPending || status.data?.downloading
        ? t("fonts.progress", { progress })
        : status.isPending ? t("fonts.loading") : status.data?.installed ? t("fonts.installed") : t("fonts.system")}</p>
      {download.isPending || status.data?.downloading ? <Progress value={progress} aria-label={t("fonts.downloading")} /> : null}
      {error ? <ErrorNotice error={error} /> : null}
    </CardContent>
    <CardFooter className="flex flex-wrap gap-2">
      {status.data?.installed ? <Button type="button" variant="outline" disabled={!!busy} onClick={() => { download.reset(); remove.mutate() }}>{t("fonts.remove")}</Button>
        : <Button type="button" variant="outline" disabled={!!busy || status.isPending || !!status.error} onClick={() => { remove.reset(); download.mutate() }}>{t("fonts.download")}</Button>}
      {status.error ? <Button type="button" variant="outline" onClick={() => void status.refetch()}>{t("setup.retry")}</Button> : null}
    </CardFooter>
  </Card>
}
