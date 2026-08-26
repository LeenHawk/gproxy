import { ChevronLeftIcon, ChevronRightIcon } from "lucide-react"
import { useTranslation } from "react-i18next"
import { Button } from "@/components/ui/button"

export function DataTablePagination({ page, pages, onPage }: { page: number; pages: number; onPage: (page: number) => void }) {
  const { t } = useTranslation()
  if (pages <= 1) return null
  return (
    <nav className="flex items-center justify-end gap-2" aria-label={t("common.dataTable.page", { page, pages })}>
      <span className="text-xs text-muted-foreground">{t("common.dataTable.page", { page, pages })}</span>
      <Button size="icon-sm" variant="outline" disabled={page <= 1} onClick={() => onPage(page - 1)} aria-label={t("common.dataTable.previous")}>
        <ChevronLeftIcon aria-hidden />
      </Button>
      <Button size="icon-sm" variant="outline" disabled={page >= pages} onClick={() => onPage(page + 1)} aria-label={t("common.dataTable.next")}>
        <ChevronRightIcon aria-hidden />
      </Button>
    </nav>
  )
}
