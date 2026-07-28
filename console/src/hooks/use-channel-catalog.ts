import { useQuery } from "@tanstack/react-query";
import { channelsQuery } from "@/api/channels";
import {
  channelMeta,
  mergeChannelCatalog,
  type ChannelMeta,
} from "@/lib/channel-meta";

export function useChannelCatalog(): ChannelMeta[] {
  const { data, isError } = useQuery({
    ...channelsQuery,
    select: mergeChannelCatalog,
  });
  return !isError && data !== undefined ? data : mergeChannelCatalog(undefined);
}

export function useChannelMeta(id: string): ChannelMeta | undefined {
  const catalog = useChannelCatalog();
  return channelMeta(id, catalog);
}
