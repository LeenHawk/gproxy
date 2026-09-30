const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { spawnSync } = require('node:child_process');

const env = process.env;
// Git's GNU tar can interpret a Windows drive letter as a remote host.
const tar = process.platform === 'win32' ? path.join(env.SystemRoot, 'System32', 'tar.exe') : 'tar';

function run(command, args, options = {}) {
  const result = spawnSync(command, args, {
    encoding: 'utf8',
    stdio: ['pipe', 'inherit', 'inherit'],
    timeout: 20 * 60 * 1000,
    ...options,
  });
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(`${command} exited with ${result.status}`);
}

function warning(error) {
  console.log(`::warning::GHCR cache: ${error.message.replace(/[\r\n]/g, ' ')}`);
}

function cargoHome() {
  return env.CARGO_HOME || path.join(os.homedir(), '.cargo');
}

function archives() {
  return [
    { name: 'target.tar.gz', root: env.GITHUB_WORKSPACE, entries: ['target'] },
    // Do not restore cargo/bin: container images and rustup own the toolchain.
    { name: 'cargo.tar.gz', root: cargoHome(), entries: ['registry', 'git'] },
  ];
}

function login(directory) {
  const config = path.join(directory, 'auth.json');
  run('oras', ['login', 'ghcr.io', '--registry-config', config,
    '--username', env.GITHUB_ACTOR, '--password-stdin'], {
    input: env.INPUT_TOKEN,
  });
  return ['--registry-config', config];
}

function cache() {
  const saving = env.STATE_ghcr_cache_post === 'true';
  if (!saving) fs.appendFileSync(env.GITHUB_STATE, 'ghcr_cache_post=true\n');
  if (saving && env.INPUT_SAVE !== 'true') return;

  // A bounded set of rolling tags: Cargo fingerprints invalidate changed Rust
  // versions, flags and dependencies. Lockfile/commit hashes in tags would keep
  // an ever-growing set of fully tagged archives alive.
  const key = `v1-${env.RUNNER_OS}-${env.RUNNER_ARCH}-${env.INPUT_KEY}`.toLowerCase();
  if (!/^[a-z0-9][a-z0-9_.-]{0,127}$/.test(key)) throw new Error('Invalid cache key');
  const repository = `${env.GITHUB_REPOSITORY.toLowerCase()}-build-cache`;
  const reference = `ghcr.io/${repository}:${key}`;
  const directory = fs.mkdtempSync(path.join(env.RUNNER_TEMP, 'ghcr-rust-cache-'));
  try {
    const auth = login(directory);
    if (saving) {
      const files = [];
      for (const archive of archives()) {
        const entries = archive.entries.filter(entry => fs.existsSync(path.join(archive.root, entry)));
        if (!entries.length) continue;
        const output = path.join(directory, archive.name);
        // Incremental state is disposable and is not needed for release reuse.
        run(tar, ['-czf', output, '--exclude=incremental', ...entries], { cwd: archive.root });
        files.push(`${archive.name}:application/vnd.gproxy.cargo-cache.v1.tar+gzip`);
        console.log(`${archive.name}: ${(fs.statSync(output).size / 1024 ** 2).toFixed(1)} MiB`);
      }
      if (!files.length) return;
      run('oras', ['push', ...auth, '--image-spec', 'v1.0',
        '--artifact-type', 'application/vnd.gproxy.cargo-cache.v1',
        '--annotation', `org.opencontainers.image.source=https://github.com/${env.GITHUB_REPOSITORY}`,
        '--annotation', `org.opencontainers.image.revision=${env.GITHUB_SHA}`,
        reference, ...files], { cwd: directory });
    } else {
      console.log(`Restoring ${reference}`);
      run('oras', ['pull', ...auth, reference, '--output', directory]);
      for (const archive of archives()) {
        const input = path.join(directory, archive.name);
        if (!fs.existsSync(input)) continue;
        fs.mkdirSync(archive.root, { recursive: true });
        run(tar, ['-xzf', input], { cwd: archive.root });
      }
    }
  } finally {
    fs.rmSync(directory, { recursive: true, force: true });
  }
}

async function cleanup() {
  // Only this cache package is managed here, never release/toolchain images.
  const [owner, repo] = env.GITHUB_REPOSITORY.split('/');
  const packageName = `${repo.toLowerCase()}-build-cache`;
  const headers = {
    Authorization: `Bearer ${env.GH_TOKEN}`,
    Accept: 'application/vnd.github+json',
    'X-GitHub-Api-Version': '2022-11-28',
  };
  async function api(endpoint, method = 'GET') {
    const response = await fetch(`${env.GITHUB_API_URL || 'https://api.github.com'}${endpoint}`, { method, headers });
    if (response.status === 404 && method === 'GET') return null;
    if (!response.ok) throw new Error(`GitHub API ${method} ${endpoint}: ${response.status}`);
    return response.status === 204 ? null : response.json();
  }
  const account = await api(`/users/${owner}`);
  if (!account) throw new Error('Repository owner not found');
  const scope = account.type === 'Organization' ? 'orgs' : 'users';
  const base = `/${scope}/${owner}/packages/container/${encodeURIComponent(packageName)}/versions`;
  const versions = [];
  for (let page = 1; ; page++) {
    const batch = await api(`${base}?per_page=100&page=${page}`);
    if (!batch) break; // The first cache upload may not have happened yet.
    versions.push(...batch);
    if (batch.length < 100) break;
  }
  const cutoff = Date.now() - 7 * 24 * 60 * 60 * 1000;
  // Collect before deleting so pagination does not skip versions. Keep a week
  // of superseded versions for in-flight pulls and manual investigation.
  for (const version of versions) {
    if (version.metadata?.container?.tags?.length !== 0) continue;
    if (Date.parse(version.updated_at || version.created_at) >= cutoff) continue;
    console.log(`Deleting old untagged cache version ${version.id}`);
    await api(`${base}/${version.id}`, 'DELETE');
  }
}

if (require.main === module) {
  if (process.argv[2] === 'cleanup') {
    cleanup().catch(error => { console.error(error); process.exitCode = 1; });
  } else {
    // Missing caches, denied fork tokens and registry outages are cold builds,
    // not release failures. Upload failures likewise leave the build intact.
    try { cache(); } catch (error) { warning(error); }
  }
}

module.exports = { cache, cleanup };
