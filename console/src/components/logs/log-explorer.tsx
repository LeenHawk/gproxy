import type { CSSProperties, Dispatch, SetStateAction } from "react"
import { useTranslation } from "react-i18next"
import { useWorkspacePaneWidth } from "@/components/workspace/use-workspace-pane-width"
import { WorkspaceResizeHandle } from "@/components/workspace/workspace-resize-handle"
import type { LogDetailDto } from "@/generated/LogDetailDto"
import type { LogPageDto } from "@/generated/LogPageDto"
import type { LogQueryDto } from "@/generated/LogQueryDto"
import type { ProviderDto } from "@/generated/ProviderDto"
import type { UserDto } from "@/generated/UserDto"
import type { UserKeyDto } from "@/generated/UserKeyDto"
import { LogDetail } from "@/components/logs/log-detail"
import { LogFilters } from "@/components/logs/log-filters"
import { LogList } from "@/components/logs/log-list"
import type { PageSize } from "@/components/data-table-state"

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
  onPageSize: (pageSize: PageSize) => void
}

export function LogExplorer(props: Props) {
  const { t } = useTranslation()
  const pane = useWorkspacePaneWidth("logs-pane-width", { defaultWidth: 480, minWidth: 320, maxWidth: 960 })
  return (
    <div className="flex flex-col gap-5">
      <LogFilters {...props} />
      <div className="flex min-w-0 flex-col gap-5 xl:flex-row xl:gap-3">
        <div style={{ "--workspace-pane-width": `${pane.width}px` } as CSSProperties} className="min-w-0 xl:w-[var(--workspace-pane-width)] xl:shrink-0">
          <LogList page={props.page} pageSize={(props.draft.limit ?? 50) as PageSize} selected={props.selected} onSelect={props.onSelect} onNext={props.onNext} onPageSize={props.onPageSize} />
        </div>
        <WorkspaceResizeHandle className="md:hidden xl:block" label={t("logs.resize")} width={pane.width} minWidth={pane.minWidth} maxWidth={pane.maxWidth} onWidthChange={pane.setWidth} onReset={pane.resetWidth} />
        <div className="min-w-0 flex-1">
          <LogDetail value={props.detail} loading={props.detailLoading} error={props.detailError} providers={props.providers} />
        </div>
      </div>
    </div>
  )
}
