import { configFamily } from "@/api/config-family"
import { CONFIGURATION_FAMILIES } from "@/api/configuration"
import type { RouteDto, RouteWrite, RoutePatch, RouteMemberDto, RouteMemberWrite, RouteMemberPatch } from "@/generated/sdk"

export const routeFamily = configFamily<RouteDto, Partial<RouteWrite>, Partial<RoutePatch>>(CONFIGURATION_FAMILIES.routes)
export const memberFamily = configFamily<RouteMemberDto, Partial<RouteMemberWrite>, Partial<RouteMemberPatch>>(CONFIGURATION_FAMILIES.routeMembers)
