//! MCP servers behind the gateway: a provider whose channel is `mcp`.

import type {
  McpScopeDto, McpScopePatch, McpScopeWrite, McpToolDto, McpToolNatureDto, McpToolNaturePatch, McpToolNatureWrite,
} from "@/generated/app"
import { batchFamily } from "@/api/admin"
import { api } from "@/api/client"

/** The provider `channel` that is a tool server rather than a model upstream. */
export const MCP_CHANNEL = "mcp"
export const MCP_TOOLS_READ = "configuration.mcp-tool-natures"
export const MCP_TOOL_CALL = "mcp.tools.call"
export const MCP_NATURES = ["read", "write", "destructive"] as const

export const mcpScopes = batchFamily<McpScopeDto, Partial<McpScopeWrite>, Partial<McpScopePatch>>("/mcp-scopes")
export const mcpToolNatures = batchFamily<McpToolNatureDto, Partial<McpToolNatureWrite>, Partial<McpToolNaturePatch>>("/mcp-tool-natures")
/** Asks the server live, so it fails without a credential or when the server is unreachable. */
export const mcpTools = (providerId: string) => api<Array<McpToolDto>>(`/admin/api/mcp-tools/${encodeURIComponent(providerId)}`)
