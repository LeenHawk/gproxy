import { useEffect, useRef, useState } from "react"
import { Copy, Eye, EyeOff, LoaderCircle } from "lucide-react"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import { Button } from "@/components/ui/button"
import { copyText } from "@/lib/copy-text"

/** Plaintext stays in this cell, never in the list query cache. */
export function KeySecretCell({ prefix, revealable, reveal }: {
  prefix: string
  revealable: boolean
  reveal: () => Promise<{ token: string }>
}) {
  const { t } = useTranslation()
  const [secret, setSecret] = useState<string | null>(null)
  const [pending, setPending] = useState(false)
  const active = useRef(false)
  useEffect(() => {
    active.current = true
    return () => { active.current = false }
  }, [])
  useEffect(() => {
    if (secret === null) return
    const timer = window.setTimeout(() => setSecret(null), 15_000)
    return () => window.clearTimeout(timer)
  }, [secret])
  const show = async () => {
    setPending(true)
    try {
      const result = await reveal()
      if (active.current) setSecret(result.token)
    } catch (error) {
      if (active.current) toast.error(error instanceof Error ? error.message : t("state.failed"))
    } finally {
      if (active.current) setPending(false)
    }
  }
  const copy = async () => {
    if (secret === null) return
    try {
      await copyText(secret)
      toast.success(t("keys.copied"))
    } catch {
      toast.error(t("keys.copyFailed"))
    }
  }
  const label = secret ? t("keys.hide") : revealable ? t("actions.reveal") : t("keys.notRetained")
  return (
    <div className="flex w-64 max-w-full items-start gap-1">
      <code className="min-w-0 flex-1 break-all whitespace-normal py-1 font-mono text-xs select-all">{secret ?? `${prefix}••••••••`}</code>
      {secret ? <Button variant="ghost" size="icon-xs" aria-label={t("actions.copy")} title={t("actions.copy")} onClick={() => void copy()}><Copy /></Button> : null}
      <Button
        variant="ghost"
        size="icon-xs"
        aria-label={label}
        title={label}
        aria-pressed={secret !== null}
        disabled={!revealable || pending}
        onClick={() => { if (secret) setSecret(null); else void show() }}
      >
        {pending ? <LoaderCircle className="animate-spin" /> : secret ? <EyeOff /> : <Eye />}
      </Button>
    </div>
  )
}
