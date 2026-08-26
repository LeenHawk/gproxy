import type { AuthCodeCompleteRequest } from "@/generated/AuthCodeCompleteRequest"
import type { AuthCodeStartRequest } from "@/generated/AuthCodeStartRequest"
import type { AuthCodeStartResponse } from "@/generated/AuthCodeStartResponse"
import type { CookieExchangeRequest } from "@/generated/CookieExchangeRequest"
import type { DevicePollRequest } from "@/generated/DevicePollRequest"
import type { DevicePollResponse } from "@/generated/DevicePollResponse"
import type { DeviceStartRequest } from "@/generated/DeviceStartRequest"
import type { DeviceStartResponse } from "@/generated/DeviceStartResponse"
import type { IdResponse } from "@/generated/IdResponse"
import { api, json } from "@/api/client"

export const startAuthcode = (value: AuthCodeStartRequest) =>
  api<AuthCodeStartResponse>("/admin/login/authcode/start", json("POST", value))

export const completeAuthcode = (value: AuthCodeCompleteRequest) =>
  api<IdResponse>("/admin/login/authcode/complete", json("POST", value))

export const startDevice = (value: DeviceStartRequest) =>
  api<DeviceStartResponse>("/admin/login/device/start", json("POST", value))

export const pollDevice = (value: DevicePollRequest) =>
  api<DevicePollResponse>("/admin/login/device/poll", json("POST", value))

export const exchangeCookie = (value: CookieExchangeRequest) =>
  api<IdResponse>("/admin/login/cookie", json("POST", value))
