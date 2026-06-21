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
| Supabase | `supabase functions deploy gproxy --project-ref "$SUPABASE_PROJECT_REF" --no-verify-jwt --network-id host`（**Docker/eszip 路径,见下方注**） | `SUPABASE_ACCESS_TOKEN` |
| EdgeOne Pages | `npx edgeone pages deploy deploy/eopages/gproxy --name <proj> -t "$EDGEONE_PAGES_API_TOKEN"` | EdgeOne Pages API token |
| Deno Deploy | `bash deploy/deno/build.sh`(**这条自带 cargo 构建 + 部署,需在有 cargo 的机器跑**) | `DENO_DEPLOY_TOKEN` |

各平台的实测记录见 `deploy/<平台>/NOTES.md`。

---

## 3. 注意

- **打包覆盖**:`build-edge` 现在打包全部 6 个平台(cloudflare / vercel / eopages / deno / netlify / supabase)+ 裸 wasm,均为 build-only、不含任何 key。
- **Supabase 必须走 Docker/eszip 路径**:`functions deploy --use-api`(无 Docker 那条)只上传 `.ts/.js`,不传 sibling `.wasm` → 运行时崩(500);改内联 base64 后 ~8.4MB 又超 `--use-api` 请求体上限(413)。**不加 `--use-api`** 走本地 Docker 把函数 bundle 成 eszip(压到 ~4.7MB)才能上。本机用 podman 顶 Docker:
  ```bash
  systemctl --user start podman.socket
  export DOCKER_HOST="unix:///run/user/$(id -u)/podman/podman.sock"
  cd deploy && supabase functions deploy gproxy --project-ref "$REF" --no-verify-jwt --network-id host
  #                                                  ^ --network-id host 绕开 WSL2 netavark/nftables 启动报错
  ```
  Supabase 的 Docker 只是**本地打包工具**,函数本身跑在它托管的 Deno 上,不是你的容器。
- **eopages / EdgeOne**:glue 用懒加载 `__gproxy_load()`,实例化推迟到首个请求,绕开 ~15s import 预算 → **未过 wasm-opt 的包也能部署**(实测 Deploy Success)。`*.edgeone.run` 域名带预览保护(`?eo_token=&eo_time=`,控制台签发)。
- **Vercel 套餐体积上限**:edge function 打包后 ~1.81MB(gzip),**Hobby 套餐上限 1MB**(Pro 2MB、Enterprise 4MB)→ Hobby 部署直接被拒。wasm-opt 那点优化压不到 1MB 以下,要发 Vercel 得上 Pro。
- **netlify**:默认 edge functions 目录是 `netlify/edge-functions`,本仓库是 `edge-functions/` → `netlify.toml` 里已加 `[build] edge_functions = "edge-functions"`,否则函数不被打包(返回静态 404)。
- **console(管理面 SPA)** 是另一套静态产物,部署形态见 `docs/deployment.md`。
