import type { ReactNode } from "react"
import { useTranslation } from "react-i18next"
import { Dialog, DialogBody, DialogContent, DialogHeader, DialogTitle } from "@/components/ui/dialog"
export function ManagementDialog({ title, children, onClose, busy = false }: { title: string; children: ReactNode; onClose: () => void; busy?: boolean }) {
  const { t } = useTranslation()
  return <Dialog open onOpenChange={open => { if (!open && !busy) onClose() }}><DialogContent className="sm:max-w-5xl" aria-describedby={undefined} closeLabel={t("actions.close")}><DialogHeader><DialogTitle>{title}</DialogTitle></DialogHeader><DialogBody><div className="flex flex-col gap-4">{children}</div></DialogBody></DialogContent></Dialog>
}
