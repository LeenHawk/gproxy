import { createContext, useContext } from "react"
import type { TFunction } from "i18next"
import type { ProviderDto } from "@/generated/sdk"

/**
 * Providers the setup wizard just made, carried across into the signed-in
 * console so their credentials can be added there: the credential forms need
 * the session's context, which only exists past the sign-in gate.
 */
export type PendingCredentials = { providers: ProviderDto[]; done: () => void }

export const PendingCredentialsContext = createContext<PendingCredentials | null>(null)

export const usePendingCredentials = () => useContext(PendingCredentialsContext)

/** The wizard's steps, the credentials one included, as the progress list names them. */
export const setupSteps = (t: TFunction) => [t("setup.connection"), t("setup.account"), t("setup.upstreams"), t("setup.credentials")]
