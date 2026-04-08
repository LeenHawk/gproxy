export type BuildInfo = {
  version: string;
  commit: string;
};

export function normalizeBuildInfo(version: string, commit: string): BuildInfo {
  const normalizedVersion = version.trim() || "dev";
  const normalizedCommit = commit.trim() || "unknown";
  return {
    version: normalizedVersion,
    commit: normalizedCommit,
  };
}

export const APP_BUILD_INFO = normalizeBuildInfo(__APP_VERSION__, __APP_COMMIT__);
