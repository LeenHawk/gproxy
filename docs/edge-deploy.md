# 边缘(wasm)部署 —— 无需 cargo

边缘平台(Cloudflare / Vercel / Netlify / EdgeOne Pages / Supabase / Deno)的构建
环境**没有 Rust/cargo 工具链**,而 `deploy/*/` 下的 wasm-bindgen glue(`_lib/` 等)
是 **gitignore 的生成产物**(clone 出来不含)。

所以部署模型是:**在有 cargo 的地方构建一次 → 把预构建产物拿去发**。

> ⚠️ 不要把平台的「连 Git 仓库自动构建」指向本仓库源码 —— 它既没有 cargo,
> 也没有 `_lib` glue,必然失败。永远部署**预构建包**。

---

## 1. 拿到预构建包

**方式 A — CI Release 产物(推荐)**
每次 GitHub Release,`release.yml` 的 `build-edge` job 产出(并附到 Release):

- `gproxy-edge-cloudflare.zip` / `gproxy-edge-vercel.zip` / `gproxy-edge-eopages.zip`
- 裸 `gproxy.wasm`(+ 每个文件的 `.sha256`)

解压后每个目录已含 `_lib/` glue + wasm,直接部署。

**方式 B — 本地构建(机器上有 cargo)**
```bash
cargo build --lib --target wasm32-unknown-unknown --release --no-default-features --features edge
bash deploy/<平台>/build.sh        # 重新生成该平台的 _lib glue(eopages 还会过 wasm-opt)
```

---

## 2. 各平台部署(平台侧零 cargo)

从预构建包所在目录执行:

| 平台 | 命令 | 凭证 |
|---|---|---|
| Cloudflare Workers | `cd cloudflare && npx wrangler deploy` | `CLOUDFLARE_API_TOKEN`(+ account id) |
| Vercel Edge | `cd vercel && npx vercel deploy --prod --token "$VERCEL_TOKEN"` | `VERCEL_TOKEN`(+ org/project) |
| Netlify Edge | `cd netlify && npx netlify deploy --prod` | `NETLIFY_AUTH_TOKEN`(+ site id) |
| Supabase | `supabase functions deploy gproxy --project-ref "$SUPABASE_PROJECT_REF"` | `SUPABASE_ACCESS_TOKEN` |
| EdgeOne Pages | `cd eopages && npx edgeone pages deploy`(详见 `deploy/eopages/NOTES.md`) | EdgeOne Pages API token |
| Deno Deploy | `bash deploy/deno/build.sh`(**这条自带 cargo 构建 + 部署,需在有 cargo 的机器跑**) | `DENO_DEPLOY_TOKEN` |

各平台的实测记录见 `deploy/<平台>/NOTES.md`。

---

## 3. 注意

- **eopages 体积**:CI 产物未过 `wasm-opt`(内联 wasm ~9.6 MB),EdgeOne Pages 有体积
  上限,自动产物可能超限 → 这个平台建议用**方式 B 本地构建**(`build.sh` 带 `wasm-opt`)
  出包再发。
- **打包覆盖**:目前 CI 只打包 cloudflare / vercel / eopages 三个 + 裸 wasm。
  deno / netlify / supabase **暂未进 Release 产物**,要发这些就用方式 B 本地构建
  (它们的 glue 也都是 gitignore 产物)。需要的话可以把它们加进 `build-edge` 打包。
- **console(管理面 SPA)** 是另一套静态产物,部署形态见 `docs/deployment.md`。
