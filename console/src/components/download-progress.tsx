import { Progress } from "@/components/ui/progress"

function size(bytes: number) {
  return bytes < 1024 * 1024 ? `${(bytes / 1024).toFixed(1)} KiB` : `${(bytes / 1024 / 1024).toFixed(1)} MiB`
}

export function DownloadProgress({ label, downloaded = 0, total }: { label: string; downloaded?: number; total?: number | null }) {
  const percent = total && total > 0 ? Math.min(100, Math.max(0, downloaded / total * 100)) : null
  return <div role="status" className="flex w-full max-w-xl flex-col gap-2">
    <div className="flex flex-wrap justify-between gap-2 text-sm">
      <span>{label}</span>
      <span className="tabular-nums">{percent !== null ? `${Math.floor(percent)}% · ` : ""}{size(downloaded)}{total ? ` / ${size(total)}` : ""}</span>
    </div>
    <Progress aria-label={label} value={percent} />
  </div>
}
