import { ChevronLeftIcon, ChevronRightIcon } from "lucide-react"
import { useId } from "react"
import { useTranslation } from "react-i18next"
import { Button } from "@/components/ui/button"
import { PAGE_SIZES, type PageSize } from "@/components/data-table-state"

export type { PageSize }
import { Field, FieldLabel } from "@/components/ui/field"
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"


type Props = {
  page: number
  pages: number
  pageSize: PageSize
  onPage: (page: number) => void
  onPageSize: (pageSize: PageSize) => void
}

export function DataTablePagination({ page, pages, pageSize, onPage, onPageSize }: Props) {
  const { t } = useTranslation()
  const pageSizeId = useId()
  return (
    <nav className="flex flex-wrap items-center justify-between gap-2" aria-label={t("common.dataTable.page", { page, pages })}>
      <Field orientation="horizontal" className="w-fit">
        <FieldLabel htmlFor={pageSizeId}>{t("common.dataTable.itemsPerPage")}</FieldLabel>
        <Select value={String(pageSize)} onValueChange={(value) => onPageSize(Number(value) as PageSize)}>
          <SelectTrigger id={pageSizeId} size="sm">
            <SelectValue />
          </SelectTrigger>
          <SelectContent align="start">
            <SelectGroup>
              {PAGE_SIZES.map((size) => <SelectItem key={size} value={String(size)}>{size}</SelectItem>)}
            </SelectGroup>
          </SelectContent>
        </Select>
      </Field>
      <div className="flex items-center gap-2">
        <span className="text-xs text-muted-foreground">{t("common.dataTable.page", { page, pages })}</span>
        <Button size="icon-sm" variant="outline" disabled={page <= 1} onClick={() => onPage(page - 1)} aria-label={t("common.dataTable.previous")}>
          <ChevronLeftIcon aria-hidden />
        </Button>
        <Button size="icon-sm" variant="outline" disabled={page >= pages} onClick={() => onPage(page + 1)} aria-label={t("common.dataTable.next")}>
          <ChevronRightIcon aria-hidden />
        </Button>
      </div>
    </nav>
  )
}
