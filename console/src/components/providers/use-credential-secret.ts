import type { ChannelFieldDto } from "@/generated/ChannelFieldDto"
import { useEffect, useRef, useState } from "react"
import { revealCredentialSecret } from "@/api/control"
import { buildSecret, isSingleKey } from "@/components/providers/credential-secret"

export function useCredentialSecret(credentialId: number | undefined, fields: Array<ChannelFieldDto>) {
  const [secretText, setSecretText] = useState("")
  const secretEdited = useRef(false)
  const storedSecret = useRef<Record<string, unknown>>({})
  const [quotaValues, setQuotaValues] = useState<Record<string, string>>({})
  const [quotaEdits, setQuotaEdits] = useState<Record<string, string>>({})
  const [revealFailed, setRevealFailed] = useState(false)
  const prefilled = useRef(false)
  useEffect(() => {
    if (credentialId === undefined || prefilled.current) return
    prefilled.current = true
    revealCredentialSecret(credentialId).then(({ secret }) => {
      if (secret === null || typeof secret !== "object" || Array.isArray(secret)) return
      const entries = Object.entries(secret)
      storedSecret.current = Object.fromEntries(entries.filter(([key]) => !key.startsWith("quota_")))
      setQuotaValues(Object.fromEntries(entries.filter(([key, value]) => key.startsWith("quota_") && typeof value === "string")) as Record<string, string>)
      if (secretEdited.current) return
      const single = isSingleKey(fields) ? storedSecret.current[fields[0].key] : undefined
      setSecretText(typeof single === "string" ? single : JSON.stringify(storedSecret.current, null, 2))
    }).catch(() => setRevealFailed(true))
  }, [credentialId, fields])

  return {
    secretText,
    changeSecret: (value: string) => { secretEdited.current = true; setSecretText(value) },
    invalid: () => secretEdited.current && secretText.trim() !== "" && buildSecret(fields, secretText) === null,
    secretValue: () => {
      // OAuth tokens may rotate after reveal; unchanged inference fields must stay server-owned.
      if (credentialId !== undefined && !secretEdited.current) return null
      const parsed = buildSecret(fields, secretText)
      if (parsed === null) return null
      return Object.fromEntries(Object.entries({ ...storedSecret.current, ...parsed }).filter(([key]) => !key.startsWith("quota_")))
    },
    quotaValues: { ...quotaValues, ...quotaEdits },
    changeQuota: (key: string, value: string) => setQuotaEdits((current) => ({ ...current, [key]: value })),
    quotaSecret: Object.keys(quotaEdits).length ? Object.fromEntries(Object.entries(quotaEdits).map(([key, value]) => [key, value.trim() || null])) : null,
    revealFailed,
  }
}
