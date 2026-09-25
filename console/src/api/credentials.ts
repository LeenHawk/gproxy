import { api, json } from "@/api/client"
import type { CredentialProviderDto } from "@/generated/app"
import type { AuthCodeStart, AuthCodeStarted, AuthCodeComplete, CredentialCreated, DeviceStart, DeviceStarted, DevicePollOutcome, CookieExchange, CredentialDto, CredentialSummaryDto, CredentialQuotaDto, CredentialLimitStatusDto, QuotaSnapshotDto, QuotaResetCreditsDto, QuotaResetWrite, QuotaResetDto, DiscoveredModelDto, ModelTestResultDto } from "@/generated/sdk"
export const credentialDirectory = () => api<CredentialProviderDto[]>("/admin/api/credentials/providers")
const path = (id: string, action: string) => `/admin/api/credentials/${encodeURIComponent(id)}/${action}`
export const revealCredential = (id: string) => api<unknown>(path(id, "reveal"), { method: "POST" })
export const credentialStatus = (id: string, status: string, reason: string | null) => api<CredentialDto>(path(id, "status"), json("POST", { status, reason }))
export const refreshCredential = (id: string, force: boolean) => api<CredentialSummaryDto>(path(id, `refresh?force=${force}`), { method: "POST" })
export const credentialQuota = (id: string) => api<CredentialQuotaDto>(path(id, "quota"))
export const credentialLimits = (id: string) => api<CredentialLimitStatusDto[]>(path(id, "limits"))
export const probeQuota = (id: string) => api<QuotaSnapshotDto>(path(id, "quota-probe"), { method: "POST" })
export const quotaResetCredits = (id: string) => api<QuotaResetCreditsDto>(path(id, "quota-reset-credits"))
export const resetUpstreamQuota = (id: string, request: QuotaResetWrite) => api<QuotaResetDto>(path(id, "quota-reset"), json("POST", request))
export const resetHealth = (id: string) => api<CredentialDto>(path(id, "health-reset"), { method: "POST" })
export const discoverWithCredential = (id: string) => api<DiscoveredModelDto[]>(path(id, "models/discover"), { method: "POST" })
export const testWithCredential = (id: string, model: string) => api<ModelTestResultDto>(path(id, "models/test"), json("POST", { model }))
const login = <T>(route: string, body: unknown) => api<T>(`/admin/api/credential-login/${route}`, json("POST", body))
export const startAuthCode = (body: AuthCodeStart) => login<AuthCodeStarted>("authcode/start", body)
export const completeAuthCode = (body: AuthCodeComplete) => login<CredentialCreated>("authcode/complete", body)
export const startDevice = (body: DeviceStart) => login<DeviceStarted>("device/start", body)
export const pollDevice = (loginSessionId: string) => login<DevicePollOutcome>("device/poll", { loginSessionId })
export const exchangeCookie = (body: CookieExchange) => login<CredentialCreated>("cookie/exchange", body)
