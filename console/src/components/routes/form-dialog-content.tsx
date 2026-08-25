import type { ReactNode } from "react"
import { DialogContent } from "@/components/ui/dialog"

export function FormDialogContent({ opener, children }: { opener: HTMLElement | null; children: ReactNode }) {
  return (
    <DialogContent
      showCloseButton={false}
      onCloseAutoFocus={(event) => {
        if (!opener) return
        event.preventDefault()
        opener.focus()
      }}
    >
      {children}
    </DialogContent>
  )
}
