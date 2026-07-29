import { useQuery } from "@tanstack/react-query";
import { channelsQuery, isLegacyChannelCatalogError } from "@/api/channels";
import {
  CHANNELS,
  channelMeta,
  mergeChannelCatalog,
  type ChannelMeta,
} from "@/lib/channel-meta";

export type ChannelCatalogAvailability = "loading" | "ready" | "legacy" | "error";

export interface ChannelCatalogState {
  catalog: ChannelMeta[];
  availability: ChannelCatalogAvailability;
  authoritative: boolean;
  error: unknown;
}

export function resolveChannelCatalog(
  data: ChannelMeta[] | undefined,
  error: unknown,
): ChannelCatalogState {
  if (data !== undefined) {
    return { catalog: data, availability: "ready", authoritative: true, error: null };
  }
  if (isLegacyChannelCatalogError(error)) {
    return { catalog: CHANNELS, availability: "legacy", authoritative: false, error: null };
  }
  if (error !== null && error !== undefined) {
    return { catalog: [], availability: "error", authoritative: false, error };
  }
  return { catalog: [], availability: "loading", authoritative: false, error: null };
}

export function useChannelCatalog(): ChannelCatalogState & {
  isFetching: boolean;
  refetch: () => Promise<unknown>;
} {
  const query = useQuery({
    ...channelsQuery,
    select: mergeChannelCatalog,
  });
  const resolved = resolveChannelCatalog(query.data, query.error);
  return {
    ...resolved,
    isFetching: query.isFetching,
    refetch: query.refetch,
  };
}

export function useChannelMeta(id: string) {
  const state = useChannelCatalog();
  return { ...state, meta: channelMeta(id, state.catalog) };
}
