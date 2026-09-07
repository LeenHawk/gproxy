import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { render, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { expect, it, vi } from "vitest"
import "@/i18n"
import type { ProviderDto } from "@/generated/ProviderDto"
import { MemberModelInput } from "./member-model-input"

it("honors refresh policy and isolates late discovery results when switching providers", async () => {
  let finishDiscovery: (value: Response) => void = () => {}
  const fetchMock = vi.fn((path: string) => {
    if (path.endsWith("/discover")) return new Promise<Response>((resolve) => { finishDiscovery = resolve })
    return Promise.resolve(new Response(JSON.stringify([
      { provider_id: 1, model_id: "saved-one" },
      { provider_id: 2, model_id: "saved-two" },
    ])))
  })
  vi.stubGlobal("fetch", fetchMock)
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  const provider = (id: number, refresh: boolean) => ({ id, settings: { auto_refresh_models: refresh } }) as ProviderDto
  const view = (id: number, refresh: boolean) => <QueryClientProvider client={client}><MemberModelInput provider={provider(id, refresh)} id="model" value="custom-model" onChange={() => {}} /></QueryClientProvider>
  const { rerender } = render(view(1, false))
  const user = userEvent.setup()
  await user.click(await screen.findByRole("combobox"))
  expect(await screen.findByRole("option", { name: "saved-one" })).toBeInTheDocument()
  expect(fetchMock.mock.calls.some(([path]) => path.endsWith("/discover"))).toBe(false)
  await user.keyboard("{Escape}")
  rerender(view(2, true))
  await waitFor(() => expect(fetchMock.mock.calls.some(([path]) => path.endsWith("/discover"))).toBe(true))
  rerender(view(1, false))
  finishDiscovery(new Response(JSON.stringify({ ok: true, models: [{ model_id: "live-two" }] })))
  await user.click(screen.getByRole("combobox"))
  expect(await screen.findByRole("option", { name: "saved-one" })).toBeInTheDocument()
  expect(screen.queryByRole("option", { name: "live-two" })).not.toBeInTheDocument()
  expect(document.getElementById("model")).toHaveValue("custom-model")
  vi.unstubAllGlobals()
})
