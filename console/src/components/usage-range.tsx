import { useTranslation } from "react-i18next"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
import { USAGE_RANGES, type UsageRange } from "@/lib/usage"

export function UsageRangeSelector({ value, onChange }: { value: UsageRange; onChange: (range: UsageRange) => void }) {
  const { t } = useTranslation()
  return (
    <ToggleGroup type="single" value={value} onValueChange={next => { if (next) onChange(next as UsageRange) }} size="sm" className="flex-wrap" aria-label={t("usage.timeRange")}>
      {(Object.keys(USAGE_RANGES) as UsageRange[]).map(range => (
        <ToggleGroupItem key={range} value={range}>{t(`range.${range}`)}</ToggleGroupItem>
      ))}
    </ToggleGroup>
  )
}
