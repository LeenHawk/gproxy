import type { ComponentProps } from "react"
import { afterEach, describe, expect, it, vi } from "vitest"
import { render, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import type { ChannelDto } from "@/generated/ChannelDto"
import type { CredentialDto } from "@/generated/CredentialDto"
import { CredentialForm } from "./credential-form"
import { Dialog, DialogContent, DialogTitle } from "@/components/ui/dialog"
import "@/i18n"
import type { ChannelFieldDto } from "@/generated/ChannelFieldDto"
import {
  buildSecret,
  defaultCredentialKind,
  fieldsForCredentialKind,
} from "./credential-secret"

const field = (key: string): ChannelFieldDto => ({
  key,
  i18n_key: key,
  control: "secret",
  required: false,
  advanced: false,
  default_value: null,
  options: [],
})

describe("credential secret shape", () => {
  it("uses the bare API key field for mixed API key and OAuth channels", () => {
    const declared = [field("api_key"), field("access_token"), field("refresh_token")]
    const fields = fieldsForCredentialKind(declared, "api_key")

    expect(defaultCredentialKind(declared)).toBe("api_key")
    expect(fields.map((item) => item.key)).toEqual(["api_key"])
    expect(buildSecret(fields, "upstream-key")).toEqual({ api_key: "upstream-key" })
  })

  it("models a cookie credential as one opaque cookie even for OAuth channels", () => {
    const fields = fieldsForCredentialKind([field("access_token"), field("refresh_token")], "cookie")

    expect(fields.map((item) => item.key)).toEqual(["cookie"])
    expect(buildSecret(fields, "sessionKey=sk-ant-sid-example")).toEqual({
      cookie: "sessionKey=sk-ant-sid-example",
    })
  })

  it("defaults cookie-only channel forms to the cookie kind", () => {
    expect(defaultCredentialKind([field("cookie")])).toBe("cookie")
  })
})

const channel: ChannelDto = {
  id: "openrouter", display_name: "OpenRouter", supports: [], routing_defaults: [], login: null,
  provider_fields: [], credential_fields: [field("api_key")], quota_fields: [field("quota_api_key"), { ...field("quota_account_id"), control: "text" }],
  endpoint_kinds: [], traffic_policy: { request_headers: [], response_headers: [], request_query: [] },
}
const credential: CredentialDto = {
  id: 7, provider_id: 3, label: "Example", kind: "api_key", quota_capabilities: null,
  version: 1, enabled: true, weight: 100, rpm_limit: null, tpm_limit: null, proxy_url: null,
  tls_fingerprint: null, invalid_tls_fingerprint: null, tls_fingerprint_error: null,
  health: "unknown", health_observed_at: null, health_response_status: null, health_detail: null, model_health: [],
}
const secretResponse = (secret: Record<string, unknown>) => new Response(JSON.stringify({ secret }), { status: 200, headers: { "content-type": "application/json" } })
function form(onSave: ComponentProps<typeof CredentialForm>["onSave"], existing: CredentialDto | null = credential, metadata = channel) {
  return <Dialog open><DialogContent><DialogTitle>Credential</DialogTitle><CredentialForm providerId={3} channel={metadata} credential={existing ?? undefined} presets={[]} onSave={onSave} onDone={vi.fn()} /></DialogContent></Dialog>
}

describe("quota query authorization", () => {
  afterEach(() => vi.unstubAllGlobals())

  it("masks query secrets and patches only the changed field, preserving inference and other query values", async () => {
    vi.stubGlobal("fetch", vi.fn<typeof fetch>().mockResolvedValue(secretResponse({ api_key: "inference value", quota_api_key: "stored query value", quota_account_id: "account scope", unknown_oauth_data: "preserved" })))
    const save = vi.fn().mockResolvedValue(undefined)
    render(form(save))
    const queryKey = screen.getByLabelText("Query API key / access token")
    await waitFor(() => expect(queryKey).toHaveValue("stored query value"))
    expect(queryKey).toHaveAttribute("type", "password")
    expect(screen.queryByText("stored query value")).not.toBeInTheDocument()
    expect(screen.getByLabelText("API key")).toHaveValue("inference value")
    expect(screen.getByLabelText("Account ID")).toHaveAttribute("type", "text")
    const user = userEvent.setup()
    await user.clear(queryKey)
    await user.type(queryKey, "replacement query value")
    await user.click(screen.getByRole("button", { name: "Save" }))
    expect(save.mock.calls[0][0]).toMatchObject({ secret: null, quota_secret: { quota_api_key: "replacement query value" } })
  })

  it("preserves unchanged query values and explicitly clears an individual field after a failed reveal", async () => {
    vi.stubGlobal("fetch", vi.fn<typeof fetch>().mockRejectedValue(new Error("unavailable")))
    const save = vi.fn().mockResolvedValue(undefined)
    render(form(save))
    await screen.findByRole("alert")
    const user = userEvent.setup()
    await user.click(screen.getByRole("button", { name: "Save" }))
    expect(save.mock.calls[0][0]).toMatchObject({ secret: null, quota_secret: null })
    await user.click(screen.getByRole("button", { name: "Clear Query API key / access token" }))
    await user.click(screen.getByRole("button", { name: "Save" }))
    expect(save.mock.calls[1][0]).toMatchObject({ secret: null, quota_secret: { quota_api_key: null } })
  })

  it("adds query authorization on creation only when declared by channel metadata", async () => {
    const save = vi.fn().mockResolvedValue(undefined)
    const { rerender } = render(form(save, null, { ...channel, quota_fields: [] }))
    expect(screen.queryByRole("group", { name: "Quota query authorization" })).not.toBeInTheDocument()
    rerender(form(save, null))
    const user = userEvent.setup()
    await user.type(screen.getByLabelText("API key"), "new inference value")
    await user.type(screen.getByLabelText("Query API key / access token"), "new query value")
    await user.click(screen.getByRole("button", { name: "Save" }))
    expect(save.mock.calls[0][0]).toMatchObject({ secret: { api_key: "new inference value" }, quota_secret: { quota_api_key: "new query value" } })
  })

  it("keeps OAuth metadata out of query patches and hides quota fields from the inference JSON", async () => {
    const metadata = { ...channel, credential_fields: [field("access_token"), field("refresh_token")] }
    vi.stubGlobal("fetch", vi.fn<typeof fetch>().mockResolvedValue(secretResponse({ access_token: "current access", refresh_token: "current refresh", other_metadata: { retained: true }, quota_api_key: "stored query" })))
    const save = vi.fn().mockResolvedValue(undefined)
    render(form(save, { ...credential, kind: "oauth" }, metadata))
    await waitFor(() => expect(screen.getByLabelText("Query API key / access token")).toHaveValue("stored query"))
    const inference = screen.getByRole("textbox", { name: "Credential JSON" })
    expect(inference).not.toHaveValue(expect.stringContaining("quota_api_key"))
    const user = userEvent.setup()
    await user.clear(inference)
    await user.type(inference, '{{"access_token":"replacement access"}')
    await user.click(screen.getByRole("button", { name: "Save" }))
    expect(save.mock.calls[0][0]).toMatchObject({ secret: { access_token: "replacement access", refresh_token: "current refresh", other_metadata: { retained: true } }, quota_secret: null })
    expect(save.mock.calls[0][0].secret).not.toHaveProperty("quota_api_key")
  })
})
