import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { render, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { expect, test, vi } from "vitest"
import "@/i18n"
import * as control from "@/api/control"
import type { InstanceSettingsDto } from "@/generated/InstanceSettingsDto"
import type { RuntimeSettingsDto } from "@/generated/RuntimeSettingsDto"
import { InstanceSettingsForm } from "./instance-settings-form"
import { INSTANCE_SETTINGS_FORM_ID, type SettingsSection } from "./instance-settings-state"

const runtime: RuntimeSettingsDto = {
  proxy: null, inherit_system_proxy: false, file_upload_max_in_flight: 0,
  cors_origins: [], trusted_proxies: [], max_attempts: 6, max_in_flight: 1024,
  log_level: "info", log_format: "text",
}
const settings: InstanceSettingsDto = {
  ...runtime,
  instance_name: "default", enable_usage: true, enable_tokenizer_vocabs: true,
  enable_tokenizer_download: false, default_tokenizer_vocab: null,
  retention_days: null, max_database_size_mb: null,
  enable_downstream_log: false, enable_downstream_log_body: false,
  enable_upstream_log: false, enable_upstream_log_body: false, disable_log_redaction: false,
  update_channel: null, enable_auto_update_check: false,
  traffic_blacklist: { request_headers: [], response_headers: [], request_query: [] },
  traffic_blacklist_defaults: { request_headers: [], response_headers: [], request_query: [] },
  runtime_status: {
    native_controls: true, effective: { ...runtime, file_upload_max_in_flight: 2 },
    overrides: [{ field: "file_upload_max_in_flight", source: "GPROXY_FILE_UPLOAD_MAX_IN_FLIGHT" }],
    log_filter: "info",
  },
}

test("section switches retain drafts and save edits from all instance sections", async () => {
  const user = userEvent.setup()
  const save = vi.spyOn(control, "saveInstanceSettings").mockImplementation(async (value) => value)
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  const view = (section: SettingsSection, current = settings) => <QueryClientProvider client={client}>
    <button type="submit" form={INSTANCE_SETTINGS_FORM_ID}>Save</button>
    <InstanceSettingsForm settings={current} section={section} />
  </QueryClientProvider>
  const page = render(view("runtime"))
  await user.clear(screen.getByLabelText("Instance name"))
  await user.type(screen.getByLabelText("Instance name"), "retained")
  expect(screen.getByText(/Effective: 2 · Startup override/)).toBeInTheDocument()
  page.rerender(view("runtime", {
    ...settings,
    runtime_status: {
      ...settings.runtime_status!,
      effective: { ...runtime, file_upload_max_in_flight: 4 },
    },
  }))
  expect(screen.getByText(/Effective: 4 · Startup override/)).toBeInTheDocument()
  expect(screen.getByLabelText("Instance name")).toHaveValue("retained")

  page.rerender(view("network"))
  expect(screen.queryByLabelText("Instance name")).not.toBeInTheDocument()
  expect(screen.queryByText("Startup overrides are active")).not.toBeInTheDocument()
  await user.click(screen.getByRole("button", { name: "Add origin" }))
  await user.type(screen.getByLabelText("Allowed browser origins 1"), "https://example.test/")

  page.rerender(view("logs"))
  expect(screen.queryByLabelText("Allowed browser origins 1")).not.toBeInTheDocument()
  await user.type(screen.getByLabelText("Retention days"), "3")
  page.rerender(view("runtime"))
  expect(screen.getByLabelText("Instance name")).toHaveValue("retained")
  await user.click(screen.getByRole("button", { name: "Save" }))
  await waitFor(() => expect(save.mock.calls[0]?.[0]).toMatchObject({
    instance_name: "retained", cors_origins: ["https://example.test/"], retention_days: 3,
  }))
  save.mockRestore()
})
