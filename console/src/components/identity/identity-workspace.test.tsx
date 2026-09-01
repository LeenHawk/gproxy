import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { render, screen } from "@testing-library/react"
import { describe, expect, it } from "vitest"
import "@/i18n"
import { IdentityWorkspace } from "@/components/keys/keys-workspace"

describe("IdentityWorkspace", () => {
  it("keeps only users, teams, and organizations at the top level", () => {
    window.history.replaceState(null, "", "/admin/identity/users")
    const client = new QueryClient()
    render(<QueryClientProvider client={client}><IdentityWorkspace organizations={[]} teams={[]} users={[]} keys={[]} providers={[]} groups={[]} permissions={[]} rateLimits={[]} quotas={[]} /></QueryClientProvider>)

    expect(screen.getAllByRole("tab").map((tab) => tab.textContent)).toEqual(["User", "Team", "Organization"])
    expect(screen.queryByRole("tab", { name: "API keys" })).toBeNull()
    expect(screen.queryByRole("tab", { name: "Access and limits" })).toBeNull()
    expect(screen.queryByRole("tab", { name: "Portal" })).toBeNull()
  })
})
