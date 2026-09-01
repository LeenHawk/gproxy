import type { Dispatch, SetStateAction } from "react"
import type { LogDetailDto } from "@/generated/LogDetailDto"
import type { LogPageDto } from "@/generated/LogPageDto"
import type { LogQueryDto } from "@/generated/LogQueryDto"
import type { ProviderDto } from "@/generated/ProviderDto"
import type { UserDto } from "@/generated/UserDto"
import type { UserKeyDto } from "@/generated/UserKeyDto"
import { LogDetail } from "@/components/logs/log-detail"
import { LogFilters } from "@/components/logs/log-filters"
import { LogList } from "@/components/logs/log-list"

type Props = {
  draft: LogQueryDto
  onDraft: Dispatch<SetStateAction<LogQueryDto>>
  onSearch: () => void
  onReset: () => void
  page: LogPageDto
  providers: Array<ProviderDto>
  users: Array<UserDto>
  keys: Array<UserKeyDto>
  selected: string | null
  onSelect: (requestId: string) => void
  detail: LogDetailDto | null
  detailLoading: boolean
  detailError: boolean
  onNext: (cursor: number) => void
}

export function LogExplorer(props: Props) {
  return (
    <div className="flex flex-col gap-5">
      <LogFilters {...props} />
      <div className="grid min-w-0 gap-5 xl:grid-cols-[minmax(20rem,0.7fr)_minmax(0,1.3fr)]">
        <LogList page={props.page} selected={props.selected} onSelect={props.onSelect} onNext={props.onNext} />
        <LogDetail value={props.detail} loading={props.detailLoading} error={props.detailError} providers={props.providers} />
      </div>
    </div>
  )
}
