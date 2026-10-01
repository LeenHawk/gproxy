const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { createHash } = require('node:crypto');
const { spawn, spawnSync } = require('node:child_process');
const { pipeline } = require('node:stream/promises');

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
  return result.stdout;
}

function warning(error) {
  console.log(`::warning::GHCR cache: ${error.message.replace(/[\r\n]/g, ' ')}`);
}

async function pipe(source, destination) {
  const processes = [source, destination].map(({ command, args, cwd }, index) => spawn(command, args, {
    cwd,
    stdio: [index === 0 ? 'ignore' : 'pipe', index === 0 ? 'pipe' : 'ignore', 'inherit'],
    timeout: 20 * 60 * 1000,
  }));
  const exited = processes.map(child => new Promise((resolve, reject) => {
    child.once('error', reject);
    child.once('close', (code, signal) => code === 0
      ? resolve()
      : reject(new Error(`${child.spawnfile} exited with ${signal || code}`)));
  }));
  try {
    await Promise.all([pipeline(processes[0].stdout, processes[1].stdin), ...exited]);
  } finally {
    for (const child of processes) if (child.exitCode === null) child.kill();
    await Promise.allSettled(exited);
  }
}

function cargoHome() {
  return env.CARGO_HOME || path.join(os.homedir(), '.cargo');
}

function archives() {
  return [
    { name: 'target', root: env.GITHUB_WORKSPACE, entries: ['target'] },
    // Do not restore cargo/bin: container images and rustup own the toolchain.
    { name: 'cargo', root: cargoHome(), entries: ['registry', 'git'] },
  ];
}

function sourceFiles() {
  // checkout's temporary safe.directory config is not shared with container
  // post actions. Trust only this job's checked-out workspace for this command.
  const tracked = run('git', ['-c', `safe.directory=${env.GITHUB_WORKSPACE}`, 'ls-files', '-z'], {
    cwd: env.GITHUB_WORKSPACE, stdio: ['ignore', 'pipe', 'inherit'],
  }).split('\0').filter(Boolean);
  const files = new Set(tracked);
  // The Console bundles are generated before restoring Rust's build cache.
  // Cargo also fingerprints these ignored files through rust-embed/Tauri.
  function generated(relative) {
    const absolute = path.join(env.GITHUB_WORKSPACE, relative);
    if (!fs.existsSync(absolute)) return;
    for (const entry of fs.readdirSync(absolute, { withFileTypes: true })) {
      const child = path.posix.join(relative, entry.name);
      if (entry.isDirectory()) generated(child);
      else if (entry.isFile()) files.add(child);
    }
  }
  generated('crates/gproxy-host-axum/assets');
  generated('crates/gproxy-host-tauri/ui');
  return [...files].filter(file => fs.existsSync(path.join(env.GITHUB_WORKSPACE, file))
    && fs.lstatSync(path.join(env.GITHUB_WORKSPACE, file)).isFile());
}

function digest(file) {
  return createHash('sha256').update(fs.readFileSync(file)).digest('hex');
}

function saveSources(directory) {
  const sources = sourceFiles().map(file => {
    const absolute = path.join(env.GITHUB_WORKSPACE, file);
    return { file, hash: digest(absolute), mtime: fs.statSync(absolute).mtimeMs / 1000 };
  });
  fs.writeFileSync(path.join(directory, 'sources.json'), JSON.stringify(sources));
}

function restoreSources(directory) {
  const snapshot = path.join(directory, 'sources.json');
  if (!fs.existsSync(snapshot)) return;
  const current = new Set(sourceFiles());
  let restored = 0;
  for (const { file, hash, mtime } of JSON.parse(fs.readFileSync(snapshot, 'utf8'))) {
    if (!current.has(file)) continue;
    const absolute = path.join(env.GITHUB_WORKSPACE, file);
    if (digest(absolute) !== hash) {
      // A changed checkout must be newer than the restored build, even if a
      // checkout tool happened to preserve the old file timestamp.
      fs.utimesSync(absolute, fs.statSync(absolute).atime, new Date());
      continue;
    }
    fs.utimesSync(absolute, fs.statSync(absolute).atime, mtime);
    restored++;
  }
  console.log(`Restored timestamps for ${restored} unchanged source files`);
}

function login(directory) {
  const config = path.join(directory, 'auth.json');
  run('oras', ['login', 'ghcr.io', '--registry-config', config,
    '--username', env.GITHUB_ACTOR, '--password-stdin'], {
    input: env.INPUT_TOKEN,
  });
  return ['--registry-config', config];
}

async function cache() {
  const saving = env.STATE_ghcr_cache_post === 'true';
  if (!saving) fs.appendFileSync(env.GITHUB_STATE, 'ghcr_cache_post=true\n');
  if (saving && env.INPUT_SAVE !== 'true') return;

  // A bounded set of rolling tags: Cargo fingerprints invalidate changed Rust
  // versions, flags and dependencies. Lockfile/commit hashes in tags would keep
  // an ever-growing set of fully tagged archives alive.
  const repository = `${env.GITHUB_REPOSITORY.toLowerCase()}-build-cache`;
  function reference(version, key) {
    const tag = `${version}-${env.RUNNER_OS}-${env.RUNNER_ARCH}-${key}`.toLowerCase();
    if (!/^[a-z0-9][a-z0-9_.-]{0,127}$/.test(tag)) throw new Error('Invalid cache key');
    return `ghcr.io/${repository}:${tag}`;
  }
  const destination = reference('v2', env.INPUT_KEY);
  const directory = fs.mkdtempSync(path.join(env.RUNNER_TEMP, 'ghcr-rust-cache-'));
  try {
    const auth = login(directory);
    if (saving) {
      const files = [];
      for (const archive of archives()) {
        const entries = archive.entries.filter(entry => fs.existsSync(path.join(archive.root, entry)));
        if (!entries.length) continue;
        const filename = `${archive.name}.tar.zst`;
        const output = path.join(directory, filename);
        const started = Date.now();
        // Incremental state is disposable and is not needed for release reuse.
        // Stream directly into parallel zstd: no second, uncompressed copy of
        // the target directory and no single-threaded gzip on the critical path.
        await pipe(
          { command: tar, args: ['--format=pax', '-cf', '-', '--exclude=incremental', ...entries], cwd: archive.root },
          { command: 'zstd', args: ['-T0', '-3', '-q', '-o', output] },
        );
        files.push(`${filename}:application/vnd.gproxy.cargo-cache.v2.tar+zstd`);
        console.log(`${filename}: ${(fs.statSync(output).size / 1024 ** 2).toFixed(1)} MiB in ${((Date.now() - started) / 1000).toFixed(1)}s`);
      }
      if (!files.length) return;
      saveSources(directory);
      files.push('sources.json:application/vnd.gproxy.cargo-sources.v1+json');
      run('oras', ['push', ...auth, '--image-spec', 'v1.0',
        '--artifact-type', 'application/vnd.gproxy.cargo-cache.v2',
        '--annotation', `org.opencontainers.image.source=https://github.com/${env.GITHUB_REPOSITORY}`,
        '--annotation', `org.opencontainers.image.revision=${env.GITHUB_SHA}`,
        destination, ...files], { cwd: directory });
    } else {
      // Keep the first zstd run warm, including jobs split from linux-checks.
      // v1 remains readable by workflows that still use the gzip action.
      const keys = [...new Set([env.INPUT_KEY, env['INPUT_RESTORE-KEY']].filter(Boolean))];
      const restoredDirectory = path.join(directory, 'restore');
      let restored = false;
      for (const key of keys) {
        for (const version of ['v2', 'v1']) {
          const source = reference(version, key);
          console.log(`Restoring ${source}`);
          try {
            fs.mkdirSync(restoredDirectory, { recursive: true });
            run('oras', ['pull', ...auth, source, '--output', restoredDirectory]);
            restored = true;
            break;
          } catch (error) {
            fs.rmSync(restoredDirectory, { recursive: true, force: true });
            console.log(`Cache unavailable: ${error.message}`);
          }
        }
        if (restored) break;
      }
      if (!restored) throw new Error('No build cache could be restored');
      for (const archive of archives()) {
        fs.mkdirSync(archive.root, { recursive: true });
        const input = path.join(restoredDirectory, `${archive.name}.tar.zst`);
        if (fs.existsSync(input)) {
          await pipe(
            { command: 'zstd', args: ['-d', '-q', '-c', input] },
            { command: tar, args: ['-xf', '-'], cwd: archive.root },
          );
        } else {
          const legacy = path.join(restoredDirectory, `${archive.name}.tar.gz`);
          if (fs.existsSync(legacy)) run(tar, ['-xzf', legacy], { cwd: archive.root });
        }
      }
      restoreSources(restoredDirectory);
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
    cache().catch(warning);
  }
}

module.exports = { cache, cleanup };
