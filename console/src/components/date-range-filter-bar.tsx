import type { ReactNode } from "react"
import { useTranslation } from "react-i18next"
import { Button } from "@/components/ui/button"
import { Card, CardContent } from "@/components/ui/card"
import { Field, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { fromLocalDateTime, toLocalDateTime, validDateRange, type DateRange } from "@/lib/date-range"

export function DateRangeFilterBar({
  range,
  onRange,
  onApply,
  onReset,
  children,
}: {
  range: DateRange
  onRange: (range: DateRange) => void
  onApply: () => void
  onReset: () => void
  children?: ReactNode
}) {
  const { t } = useTranslation()
  const valid = validDateRange(range)
  return (
    <Card size="sm">
      <CardContent className="flex flex-col gap-3">
        <FieldGroup className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
          <Field>
            <FieldLabel htmlFor="range-start">{t("common.filters.start")}</FieldLabel>
            <Input
              id="range-start"
              type="datetime-local"
              value={toLocalDateTime(range.start)}
              max={toLocalDateTime(range.end)}
              aria-invalid={!valid}
              onChange={(event) => onRange({ ...range, start: fromLocalDateTime(event.target.value) })}
            />
          </Field>
          <Field>
            <FieldLabel htmlFor="range-end">{t("common.filters.end")}</FieldLabel>
            <Input
              id="range-end"
              type="datetime-local"
              value={toLocalDateTime(range.end)}
              min={toLocalDateTime(range.start)}
              aria-invalid={!valid}
              onChange={(event) => onRange({ ...range, end: fromLocalDateTime(event.target.value) })}
            />
          </Field>
          {children}
        </FieldGroup>
        <div className="flex items-center justify-end gap-2">
          {!valid ? <p className="mr-auto text-xs text-destructive">{t("common.filters.invalidRange")}</p> : null}
          <Button variant="outline" onClick={onReset}>{t("common.filters.reset")}</Button>
          <Button onClick={onApply} disabled={!valid}>{t("common.filters.apply")}</Button>
        </div>
      </CardContent>
    </Card>
  )
}
