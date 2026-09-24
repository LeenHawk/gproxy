//! Destructive actions name their target and ask for confirmation.

import { useState, type ReactNode } from "react"
import { useTranslation } from "react-i18next"
import {
  AlertDialog, AlertDialogAction, AlertDialogCancel, AlertDialogContent,
  AlertDialogFooter, AlertDialogHeader, AlertDialogTitle,
} from "@/components/ui/alert-dialog"
import { Button } from "@/components/ui/button"

export function ConfirmButton({ title, confirmLabel, onConfirm, children, disabled, iconOnly = false }: {
  title: string
  confirmLabel?: string
  onConfirm: () => void
  children: ReactNode
  disabled?: boolean
  iconOnly?: boolean
}) {
  const { t } = useTranslation()
  const [open, setOpen] = useState(false)
  return (
    <>
      <Button variant="ghost" size={iconOnly ? "icon-sm" : "sm"} title={iconOnly ? title : undefined} aria-label={iconOnly ? title : undefined} disabled={disabled} onClick={() => setOpen(true)}>{children}</Button>
      <AlertDialog open={open} onOpenChange={setOpen}>
        <AlertDialogContent aria-describedby={undefined}>
          <AlertDialogHeader>
            <AlertDialogTitle className="wrap-anywhere">{title}</AlertDialogTitle>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{t("actions.cancel")}</AlertDialogCancel>
            <AlertDialogAction variant="destructive" onClick={() => { setOpen(false); onConfirm() }}>
              {confirmLabel ?? t("actions.delete")}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  )
}
