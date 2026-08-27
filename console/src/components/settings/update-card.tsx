import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import { applyUpdate, rollbackUpdate, updateStatus } from "@/api/native"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardDescription, CardFooter, CardHeader, CardTitle } from "@/components/ui/card"
import { QueryState } from "@/components/query-state"

export function UpdateCard() {
  const { t } = useTranslation()
  const queryClient = useQueryClient()
  const query = useQuery({ queryKey: ["native-update"], queryFn: updateStatus, retry: false })
  const apply = useMutation({
    mutationFn: applyUpdate,
    onSuccess: async (result) => {
      await queryClient.invalidateQueries({ queryKey: ["native-update"] })
      toast.success(t("settings.update.applied", { version: result.version }))
    },
    onError: () => toast.error(t("settings.update.applyError")),
  })
  const rollback = useMutation({
    mutationFn: rollbackUpdate,
    onSuccess: () => toast.success(t("settings.update.rolledBack")),
    onError: () => toast.error(t("settings.update.rollbackError")),
  })
  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("settings.update.title")}</CardTitle>
        <CardDescription>{t("settings.update.description")}</CardDescription>
      </CardHeader>
      <CardContent>
        <QueryState loading={query.isLoading} error={query.error ? t("settings.update.loadError") : ""}>
          {query.data ? <div className="grid gap-3 text-sm">
            <p>{t("settings.update.versions", { current: query.data.current, latest: query.data.latest })}</p>
            <p className="text-muted-foreground">{t("settings.update.target", { channel: query.data.channel, target: query.data.target, restart: query.data.restart })}</p>
            {query.data.notes ? <pre className="max-h-64 overflow-auto whitespace-pre-wrap rounded-lg border bg-muted/35 p-3 font-sans text-xs">{query.data.notes}</pre> : null}
          </div> : null}
        </QueryState>
      </CardContent>
      {query.data ? <CardFooter className="flex-wrap justify-end gap-2">
        <Button variant="outline" disabled={!query.data.rollback_available || rollback.isPending} onClick={() => rollback.mutate()}>{t("settings.update.rollback")}</Button>
        <Button disabled={!query.data.available || apply.isPending} onClick={() => apply.mutate()}>{t(query.data.available ? "settings.update.apply" : "settings.update.current")}</Button>
      </CardFooter> : null}
    </Card>
  )
}
