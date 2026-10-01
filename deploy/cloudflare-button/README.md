# Deploy GPROXY to Cloudflare

[![Deploy to Cloudflare](https://deploy.workers.cloudflare.com/button)](https://deploy.workers.cloudflare.com/?url=https://github.com/LeenHawk/gproxy/tree/dev/deploy/cloudflare-button)

This standalone template downloads the official **v4.0.1** Worker and console,
checks the release SHA-256, and deploys with D1. No Rust build is needed.

1. Click the button and connect your GitHub or GitLab account.
2. Choose the Worker and D1 names. Set `GPROXY_ADMIN_PASSWORD` (at least 8 characters)
   and `GPROXY_MASTER_KEY` (a saved 32-byte key, e.g. `openssl rand -hex 32`).
3. Keep the detected build (`npm run build`) and deploy (`npm run deploy`) commands.
4. After deployment, open `/console/` and sign in as `admin`. Add providers and
   create a gateway API key in the console.

The template pins its release in `prepare-release.mjs`. Keep the D1 database and
master key when updating. On startup, the administrator password secret overrides the stored password
for `admin` (or `GPROXY_ADMIN_USER`). An unchanged password preserves sessions;
a changed password ends them. Removing the secret keeps the stored password.

For local validation: `npm install`, `npm run build`, then `npm run check`.
A dry run checks packaging, not Cloudflare account limits or live upstream access.

[Deployment guide](https://gproxy.leenhawk.com/deployment/edge/) ·
[中文部署说明](https://gproxy.leenhawk.com/zh-cn/deployment/edge/)
