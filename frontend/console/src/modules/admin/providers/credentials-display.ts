function asText(value: unknown): string {
  return typeof value === "string" ? value.trim() : "";
}

function shortId(value: string, length = 8): string {
  return value.slice(0, Math.max(1, length));
}

export function summarizeCredential(credential: Record<string, unknown>): {
  primary: string;
  secondary: string[];
} {
  const accountId = asText(credential.account_id);
  if (accountId) {
    const secondary = [asText(credential.user_email)].filter(Boolean);
    return {
      primary: shortId(accountId),
      secondary,
    };
  }

  const accountUuid = asText(credential.account_uuid);
  if (accountUuid) {
    const secondary = [
      asText(credential.rate_limit_tier),
      asText(credential.user_email),
    ].filter(Boolean);
    return {
      primary: shortId(accountUuid),
      secondary,
    };
  }

  const projectId = asText(credential.project_id);
  if (projectId) {
    const secondary = [asText(credential.user_email)].filter(Boolean);
    return {
      primary: projectId,
      secondary,
    };
  }

  const userEmail = asText(credential.user_email);
  if (userEmail) {
    return {
      primary: userEmail,
      secondary: [],
    };
  }

  const deviceId = asText(credential.device_id);
  if (deviceId) {
    return {
      primary: shortId(deviceId),
      secondary: [],
    };
  }

  const apiKey = asText(credential.api_key);
  if (apiKey) {
    return {
      primary: shortId(apiKey),
      secondary: [],
    };
  }

  return {
    primary: "Credential configured",
    secondary: [],
  };
}
