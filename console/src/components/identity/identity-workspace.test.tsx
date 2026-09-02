import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { render, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { afterEach, describe, expect, it, vi } from "vitest"
import "@/i18n"
import { IdentityWorkspace } from "@/components/keys/keys-workspace"

describe("IdentityWorkspace", () => {
  afterEach(() => vi.unstubAllGlobals())

  it("keeps only users, teams, and organizations at the top level", () => {
    window.history.replaceState(null, "", "/admin/identity/users")
    const client = new QueryClient()
    render(<QueryClientProvider client={client}><IdentityWorkspace organizations={[]} teams={[]} users={[]} keys={[]} providers={[]} groups={[]} permissions={[]} rateLimits={[]} quotas={[]} /></QueryClientProvider>)

    expect(screen.getAllByRole("tab").map((tab) => tab.textContent)).toEqual(["User", "Team", "Organization"])
    expect(screen.queryByRole("tab", { name: "API keys" })).toBeNull()
    expect(screen.queryByRole("tab", { name: "Access and limits" })).toBeNull()
    expect(screen.queryByRole("tab", { name: "Portal" })).toBeNull()
  })

  it("updates a user from the profile pane and changes the password separately", async () => {
    window.history.replaceState(null, "", "/admin/identity/users/7/profile")
    const fetchMock = vi.fn<typeof fetch>().mockResolvedValue(new Response(null, { status: 204 }))
    vi.stubGlobal("fetch", fetchMock)
    const client = new QueryClient()
    render(<QueryClientProvider client={client}><IdentityWorkspace
      organizations={[{ id: 2, name: "Acme", enabled: true }]}
      teams={[{ id: 3, organization_id: 2, name: "Platform", enabled: true }]}
      users={[{ id: 7, name: "Alice", organization_id: 2, team_id: 3, enabled: true, is_admin: true }]}
      keys={[]}
      providers={[]}
      groups={[]}
      permissions={[]}
      rateLimits={[]}
      quotas={[]}
    /></QueryClientProvider>)

    const user = userEvent.setup()
    await user.clear(screen.getByRole("textbox", { name: "Name" }))
    await user.type(screen.getByRole("textbox", { name: "Name" }), "Alice Renamed")
    await user.type(screen.getByLabelText("Password"), "new password")
    await user.click(screen.getByRole("button", { name: "Save" }))

    await waitFor(() => expect(fetchMock).toHaveBeenCalledTimes(2))
    expect(fetchMock.mock.calls[0]?.[0]).toBe("/admin/api/users/7")
    expect(JSON.parse(String(fetchMock.mock.calls[0]?.[1]?.body))).toEqual({
      name: "Alice Renamed",
      organization_id: 2,
      team_id: 3,
      enabled: true,
      is_admin: true,
      password: null,
    })
    expect(fetchMock.mock.calls[1]?.[0]).toBe("/admin/api/users/7/password")
    expect(JSON.parse(String(fetchMock.mock.calls[1]?.[1]?.body))).toEqual({ password: "new password" })
  })
})
