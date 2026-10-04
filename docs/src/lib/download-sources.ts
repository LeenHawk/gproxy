export const downloadSources = {
  github: {
    label: 'GitHub',
    releases: 'https://github.com/LeenHawk/gproxy/releases',
    latest: 'https://github.com/LeenHawk/gproxy/releases/latest',
    nightly: 'https://github.com/LeenHawk/gproxy/releases/tag/nightly',
  },
  cnb: {
    label: 'CNB',
    releases: 'https://cnb.cool/LeenHawk/gproxy/-/releases',
    latest: 'https://cnb.cool/LeenHawk/gproxy/-/releases',
    nightly: 'https://cnb.cool/LeenHawk/gproxy/-/releases/nightly',
  },
  gitlab: {
    label: 'GitLab',
    releases: 'https://gitlab.com/leenhawk1/gproxy/-/releases',
    latest: 'https://gitlab.com/leenhawk1/gproxy/-/releases',
    nightly: 'https://gitlab.com/leenhawk1/gproxy/-/releases/nightly',
  },
} as const;

export type DownloadSource = keyof typeof downloadSources;
export type Release = {
  tag_name: string;
  html_url: string;
  published_at: string;
  assets: { name: string; browser_download_url: string; size?: number }[];
};

export async function loadRelease(source: DownloadSource, signal: AbortSignal): Promise<Release> {
  const json = async (url: string) => {
    const response = await fetch(url, { signal });
    if (!response.ok) throw new Error(`Release request failed: ${response.status}`);
    return response.json();
  };
  if (source === 'github') {
    return json('https://api.github.com/repos/LeenHawk/gproxy/releases/latest');
  }

  const base = downloadSources[source].releases;
  // The moving "release" tag contains only the manifest, not the packages.
  const gitlabApi = 'https://gitlab.com/api/v4/projects/leenhawk1%2Fgproxy/releases';
  const pointer = source === 'gitlab' ? await json(`${gitlabApi}/release`) : undefined;
  const manifestUrl = source === 'cnb'
    ? `${base}/download/release/manifest.json`
    : pointer.assets.links.find((asset: { name: string }) => asset.name === 'manifest.json')?.url;
  if (!manifestUrl) throw new Error('Missing release manifest');
  const manifest = await json(manifestUrl);
  const tag = `v${manifest.version}`;
  const encodedTag = encodeURIComponent(tag);
  if (source === 'gitlab') {
    const release = await json(`${gitlabApi}/${encodedTag}`);
    return {
      tag_name: tag,
      html_url: `${base}/${encodedTag}`,
      published_at: release.released_at,
      assets: release.assets.links.map((asset: { name: string; direct_asset_url: string }) => ({
        name: asset.name,
        browser_download_url: asset.direct_asset_url,
      })),
    };
  }
  // CNB's metadata API requires authentication. Its public checksum attachment
  // lists the published packages, including formats absent from the updater manifest.
  const assetBase = `${base}/download/${encodedTag}`;
  const response = await fetch(`${assetBase}/SHA256SUMS`, { signal });
  if (!response.ok) throw new Error(`Checksum request failed: ${response.status}`);
  const names = [...(await response.text()).matchAll(/^[a-f0-9]{64} {2}([^\r\n]+)$/gm)].map((match) => match[1]);
  if (!names.length) throw new Error('Empty release checksum list');
  const sizes = new Map<string, number>(manifest.artifacts.map((asset: { url: string; size: number }) => [
    new URL(asset.url).pathname.split('/').pop(), asset.size,
  ]));
  return {
    tag_name: tag,
    html_url: `${base}/${encodedTag}`,
    published_at: '',
    assets: [...names, 'SHA256SUMS', 'manifest.json'].map((name) => ({
      name,
      browser_download_url: `${assetBase}/${encodeURIComponent(name)}`,
      size: sizes.get(name),
    })),
  };
}
