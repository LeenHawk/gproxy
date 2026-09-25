import { cn } from "@/lib/utils"
import type { ReactNode } from "react"
import { useTranslation } from "react-i18next"
import { Dialog, DialogBody, DialogContent, DialogHeader, DialogTitle } from "@/components/ui/dialog"
export function ManagementDialog({ title, titleAside, children, onClose, busy = false, className }: { title: string; titleAside?: ReactNode; children: ReactNode; onClose: () => void; busy?: boolean; className?: string }) {
  const { t } = useTranslation()
  return <Dialog open onOpenChange={open => { if (!open && !busy) onClose() }}><DialogContent className={cn("sm:max-w-5xl", className)} aria-describedby={undefined} closeLabel={t("actions.close")}><DialogHeader>{titleAside ? <div className="flex min-w-0 items-start justify-between gap-3"><DialogTitle className="min-w-0 flex-1">{title}</DialogTitle><div className="shrink-0">{titleAside}</div></div> : <DialogTitle>{title}</DialogTitle>}</DialogHeader><DialogBody><div className="flex flex-col gap-4">{children}</div></DialogBody></DialogContent></Dialog>
}
