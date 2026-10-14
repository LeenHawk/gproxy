# Docker Hub 发布

目标镜像：`docker.io/leenhawk/gproxy`。

Release workflow 的 `publish` job 在已有发布步骤完成后，将本次构建的
GHCR OCI indexes 复制到 Docker Hub，复用同一发布锁，不重新构建镜像。
GNU 和 musl 镜像均保留 amd64、arm64、riscv64 架构及 attestations。

| 来源 | GNU 标签 | musl 标签 |
| --- | --- | --- |
| `dev` | `nightly` | `nightly-musl` |
| `main` | `staging` | `staging-musl` |
| 稳定版本标签 | `vX.Y.Z`、`staging` | `vX.Y.Z-musl`、`staging-musl` |
| 预发布版本标签 | 原版本标签 | 原版本标签加 `-musl` |

GitHub Actions 使用 `release` environment 的 `DOCKERHUB_TOKEN` secret，
登录用户名为 `leenhawk`。Token 需要目标仓库的写权限。
本地 token 保存在 `dev_docs/.dockerhub-token`（权限 `0600`，由 `.gitignore`
的隐藏文件规则排除），不得加入 Git。

更新 CI secret：

```bash
gh secret set DOCKERHUB_TOKEN --env release < dev_docs/.dockerhub-token
```
