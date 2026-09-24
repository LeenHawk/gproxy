import type { AdminContextDto, AdminScopeDto, PortalContextDto } from "@/generated/app"
export const instanceScope: AdminScopeDto = { kind: "instance", id: null, name: null, organizationId: null, selector: "instance", current: true }
export const orgScope: AdminScopeDto = { kind: "organization", id: "o1", name: "Acme", organizationId: "o1", selector: "organization:o1", current: true }
export function adminContext(sections: string[], scope: AdminScopeDto = instanceScope): AdminContextDto {
  return { user: { id: "u1", name: "admin", role: "admin", instanceAdmin: scope.kind === "instance" }, callerKind: "session", scopeHeader: "x-gproxy-admin-scope", scope, scopes: [scope], sections: sections.map(id => ({ id, path: `/${id}`, capabilities: ["read", "write"] })) }
}
export function portalContext(overrides: Partial<PortalContextDto> = {}): PortalContextDto {
  return { user: { id: "u1", name: "ada", role: "user", hasPassword: true }, organizations: [], teams: [], features: { canCreateKeys: true, canChangePassword: true, canSeeLogs: true, canSeeConsole: true }, ...overrides }
}
