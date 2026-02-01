# gproxy

gproxy 是一个用 Rust 编写的多渠道 AI 代理服务，带内嵌的管理后台。

## 特性
- 多渠道路由与凭证管理
- 管理 API（providers / credentials / users / keys）
- Usage 统计与部分上游的 usage 直连展示
- 内嵌 SPA 管理界面（React + Tailwind）

## 快速开始
```bash
cargo run --release -- --admin-key your-admin-key
```
打开管理界面：
```
http://127.0.0.1:8787/
```

## 配置说明
gproxy 的配置存储在数据库中，默认使用 `./data` 下的 SQLite。

CLI 参数：
- `--host <ip>`（默认 `127.0.0.1`）
- `--port <port>`（默认 `8787`）
- `--admin-key <key>`（默认 `pwd`）
- `--dsn <dsn>`（可选，例如 `sqlite:///path/to/gproxy.db`）
- `--data-dir <dir>`（默认 `./data`）
- `--proxy <url>`（可选，上游代理）

环境变量：
- `GPROXY_DATA_DIR`（设置数据目录）

管理 API 认证：
- `x-admin-key: <admin_key>` 或 `Authorization: Bearer <admin_key>`

更多路由与示例见 `route.md`。

## 前端
管理界面位于 `apps/gproxy/frontend`，构建产物会内嵌到
`apps/gproxy/frontend/dist`。

## Docker
构建：
```bash
docker build -t gproxy:local .
```
运行：
```bash
docker run --rm -p 8787:8787 \
  -e GPROXY_ADMIN_KEY=your-admin-key \
  -e GPROXY_HOST=0.0.0.0 \
  -e GPROXY_PORT=8787 \
  -e GPROXY_DATA_DIR=/app/data \
  -v $(pwd)/data:/app/data \
  gproxy:local
```

## License
AGPL-3.0-or-later
