//! The one moment a key's plaintext exists in a browser.
//!
//! `ApiKeyCreated.token` and `PortalKeyCreated.token` are returned by the mint
//! and by a rotation and by nothing else — unless the key was created with
//! `retainSecret`, in which case an explicit, audited reveal can produce it
//! again. So this dialog is modal and says so: closing it is the last chance.

import { useState } from "react"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import { Button } from "@/components/ui/button"
import {
  Dialog, DialogBody, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle,
} from "@/components/ui/dialog"
import { copyText } from "@/lib/copy-text"

export function SecretDialog({ token, onClose, description }: {
  token: string | null
  onClose: () => void
  description?: string
}) {
  const { t } = useTranslation()
  const [copied, setCopied] = useState(false)
  return (
    <Dialog open={token !== null} onOpenChange={(open) => { if (!open) { setCopied(false); onClose() } }}>
      <DialogContent className="sm:max-w-lg" closeLabel={t("actions.close")}>
        <DialogHeader>
          <DialogTitle>{t("keys.secretTitle")}</DialogTitle>
          <DialogDescription>{description ?? t("keys.secretHint")}</DialogDescription>
        </DialogHeader>
        <DialogBody>
          <code className="block w-full rounded-lg border border-border bg-muted/40 p-3 font-mono text-xs break-all select-all">
            {token}
          </code>
        </DialogBody>
        <DialogFooter>
          <Button
            variant="outline"
            onClick={async () => {
              if (!token) return
              try {
                await copyText(token)
                setCopied(true)
                toast.success(t("keys.copied"))
              } catch {
                toast.error(t("keys.copyFailed"))
              }
            }}
          >
            {copied ? t("actions.copied") : t("actions.copy")}
          </Button>
          <Button onClick={() => { setCopied(false); onClose() }}>{t("actions.done")}</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
