import { Activity, Gauge, KeyRound, UserCog } from "lucide-react";
import type { NavItem } from "./nav";

// labelKey uses the "portal:" ns prefix so NavList's t(labelKey) resolves from the portal namespace
export const PORTAL_NAV: NavItem[] = [
  { to: "/account/keys",     icon: KeyRound, labelKey: "portal:nav.keys" },
  { to: "/account/usage",    icon: Activity, labelKey: "portal:nav.usage" },
  { to: "/account/limits",   icon: Gauge,    labelKey: "portal:nav.limits" },
  { to: "/account/security", icon: UserCog,  labelKey: "portal:nav.account" },
];
