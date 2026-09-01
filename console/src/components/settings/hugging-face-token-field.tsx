import type { TokenizerAuthDto } from "@/generated/TokenizerAuthDto"
import { useMutation, useQueryClient } from "@tanstack/react-query"
import { EyeIcon, EyeOffIcon } from "lucide-react"
import { useState } from "react"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import { revealTokenizerAuth, updateTokenizerAuth } from "@/api/control"
import { Button } from "@/components/ui/button"
import { Field, FieldDescription, FieldLabel } from "@/components/ui/field"
import { InputGroup, InputGroupAddon, InputGroupButton, InputGroupInput } from "@/components/ui/input-group"

export function HuggingFaceTokenField({ auth }: { auth: TokenizerAuthDto }) {
  const { t } = useTranslation()
  const client = useQueryClient()
  const [token, setToken] = useState("")
  const [visible, setVisible] = useState(false)
  const update = useMutation({
    mutationFn: updateTokenizerAuth,
    onSuccess: async (next) => {
      setToken("")
      setVisible(false)
      client.setQueryData(["tokenizer-auth"], next)
      await client.invalidateQueries({ queryKey: ["tokenizer-auth"] })
      toast.success(t("settings.tokenizers.authSaved"))
    },
    onError: () => toast.error(t("settings.tokenizers.authError")),
  })
  const reveal = useMutation({
    mutationFn: revealTokenizerAuth,
    onSuccess: (value) => { setToken(value.token); setVisible(true) },
    onError: () => toast.error(t("settings.tokenizers.authRevealError")),
  })
  const toggle = () => {
    if (visible) return setVisible(false)
    if (token) return setVisible(true)
    if (auth.configured) reveal.mutate()
  }
  const pending = update.isPending || reveal.isPending

  return (
    <form className="flex flex-col gap-3" onSubmit={(event) => { event.preventDefault(); const value = token.trim(); if (value) update.mutate({ token: value }) }}>
      <Field>
        <FieldLabel htmlFor="hugging-face-token">{t("settings.tokenizers.authToken")}</FieldLabel>
        <InputGroup>
          <InputGroupInput
            id="hugging-face-token"
            type={visible ? "text" : "password"}
            autoComplete="off"
            className="font-mono"
            value={token}
            placeholder={auth.configured ? t("settings.tokenizers.authConfigured") : t("settings.tokenizers.authPlaceholder")}
            disabled={pending}
            onChange={(event) => setToken(event.target.value)}
          />
          <InputGroupAddon align="inline-end">
            <InputGroupButton type="button" size="icon-xs" aria-label={t(visible ? "settings.tokenizers.authHide" : "settings.tokenizers.authShow")} disabled={pending || (!token && !auth.configured)} onClick={toggle}>
              {visible ? <EyeOffIcon aria-hidden /> : <EyeIcon aria-hidden />}
            </InputGroupButton>
          </InputGroupAddon>
        </InputGroup>
        <FieldDescription>{t("settings.tokenizers.authHint")}</FieldDescription>
      </Field>
      <div className="flex justify-end gap-2">
        {auth.configured ? <Button type="button" size="sm" variant="outline" disabled={pending} onClick={() => update.mutate({ token: null })}>{t("settings.tokenizers.authClear")}</Button> : null}
        <Button type="submit" size="sm" disabled={pending || !token.trim()}>{t(update.isPending ? "settings.tokenizers.authSaving" : "settings.tokenizers.authSave")}</Button>
      </div>
    </form>
  )
}
