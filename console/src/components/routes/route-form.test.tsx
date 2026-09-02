import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { render, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { afterEach, describe, expect, it, vi } from "vitest"
import "@/i18n"
import { RouteEditor } from "@/components/routes/route-form"

describe("RouteEditor", () => {
  afterEach(() => vi.unstubAllGlobals())

  it("updates the selected route inline", async () => {
    const fetchMock = vi.fn<typeof fetch>().mockResolvedValue(new Response(null, { status: 204 }))
    vi.stubGlobal("fetch", fetchMock)
    const changed = vi.fn()
    const client = new QueryClient()
    render(<QueryClientProvider client={client}><RouteEditor route={{ id: 4, name: "Primary", max_attempts: 3, enabled: true }} onChanged={changed} /></QueryClientProvider>)

    const user = userEvent.setup()
    await user.clear(screen.getByRole("textbox", { name: "Route name" }))
    await user.type(screen.getByRole("textbox", { name: "Route name" }), "Primary Route")
    await user.clear(screen.getByRole("spinbutton", { name: "Maximum attempts" }))
    await user.type(screen.getByRole("spinbutton", { name: "Maximum attempts" }), "5")
    await user.click(screen.getByRole("button", { name: "Save" }))

    await waitFor(() => expect(fetchMock).toHaveBeenCalledOnce())
    expect(fetchMock.mock.calls[0]?.[0]).toBe("/admin/api/routes/4")
    expect(JSON.parse(String(fetchMock.mock.calls[0]?.[1]?.body))).toEqual({ name: "Primary Route", max_attempts: 5, enabled: true })
    expect(changed).toHaveBeenCalledOnce()
  })
})
