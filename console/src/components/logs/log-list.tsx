import { useTranslation } from "react-i18next"
import type { LogPageDto } from "@/generated/LogPageDto"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import { formatInstant } from "@/lib/format"
import { cn } from "@/lib/utils"

export function LogList({ page, selected, onSelect, onNext }: { page: LogPageDto; selected: string | null; onSelect: (requestId: string) => void; onNext: (cursor: number) => void }) {
  const { t, i18n } = useTranslation()
  return (
    <Card size="sm" className="min-w-0">
      <CardHeader><CardTitle>{t("logs.list.title")}</CardTitle></CardHeader>
      <CardContent className="flex flex-col gap-2">
        {page.items.length === 0 ? <p className="py-8 text-center text-sm text-muted-foreground">{t("logs.list.empty")}</p> : page.items.map((item) => (
          <button key={item.id} type="button" aria-pressed={selected === item.request_id} onClick={() => onSelect(item.request_id)} className={cn("grid min-w-0 gap-1 rounded-lg border p-3 text-left transition-colors hover:bg-muted/60", selected === item.request_id && "border-primary bg-muted")}>
            <span className="flex items-center justify-between gap-3"><span className="truncate font-medium">{item.method} {item.path}</span><span className={cn("font-mono text-xs", item.response_status != null && item.response_status >= 400 ? "text-destructive" : "text-muted-foreground")}>{item.response_status ?? t("logs.pending")}</span></span>
            <span className="truncate font-mono text-xs text-muted-foreground">{item.request_id}</span>
            <span className="text-xs text-muted-foreground">{formatInstant(item.at, i18n.language)}</span>
          </button>
        ))}
        {page.next_cursor != null ? <Button variant="outline" onClick={() => onNext(page.next_cursor!)}>{t("logs.list.next")}</Button> : null}
      </CardContent>
    </Card>
  )
}
