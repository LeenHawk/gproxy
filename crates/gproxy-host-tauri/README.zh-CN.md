# gproxy-host-tauri

[English](README.md) | 简体中文

GPROXY v4 的桌面宿主。一个进程、一个实例、两个入口：

```
         窗口                        Claude Code、Codex CLI
          │                                    │
      Tauri IPC                          HTTP，127.0.0.1
          │                                    │
  ipc::table（本 crate）        gproxy-host-axum，只挂数据面
          └──────────────┬─────────────────────┘
                   同一个 gproxy_app::App
               一份 Gproxy · 一份快照 · 一份 cache
```

逻辑几乎全在库里。`gproxy-desktop` 这个二进制只负责开窗并调 `run()`，不解析
也不决定任何东西——正因如此，测试才能在没有显示服务器的机器上把整套装配跑
完。

```rust,ignore
use gproxy_host_tauri::{Desktop, secrets::Keychain};

let desktop = Desktop::start(data_dir, &Keychain).await?;
println!("data plane on {}", desktop.data_plane().base_url);
desktop.shutdown();
```

## 数据面不走 IPC

数据面的客户端是别的程序，它们只会说 HTTP，连不上 Tauri IPC，以后也不会。
所以桌面进程在 `127.0.0.1` 上给它们跑一份真正的宿主——
[`gproxy-host-axum`](../gproxy-host-axum)——用的是 IPC 命令用的同一个 `App`。
本 crate **不实现**数据面，它只是把已有的那个托在自己进程里。

关于那个 socket，有两件事：

**它照样要 key。** IPC 通道本身就是信任边界，回环 socket 不是。同机任何进程
都能连上去；一个不鉴权的数据面等于让它们随便花用户的上游额度。同一套
authenticator、同一套准入、同一个 `401`。

**它只提供数据面。** `/admin/api` 和 `/portal/api` 返回 `404`，body 里写明
它们去哪了。服务器需要这两条路由——运维不在机器跟前时靠浏览器进来——但桌面壳
有窗口，用不到的面就不该可达。这也让网关 key 只是"能用这个实例"的凭据，而不是
一把管理员钥匙。

### 端口

默认 `7071`，比服务器的 `7070` 大一，两者可以并存。**固定而非随机**：客户端
里填的是一个填一次就不再动的 base URL，端口每次启动都变意味着每次启动都要
改一遍所有客户端。要改就在 `gproxy.toml` 里写 `port`。

## 配置

两个来源，不是命令行那五个。窗口没有参数、没有值得读的 shell 环境，也没有
`.env`。

1. 数据目录下的 `gproxy.toml`——就是服务器读的那套 `AppConfig` schema，不另起
   一份文档；
2. 桌面自己的默认值。

有三个字段归壳管、不从文件读，因为桌面实例在这三件事上没有自由：`data_dir` 是
平台的应用数据目录（Tauri 给的），store 是里面的一个 SQLite 文件，控制台就是
窗口本身而不是 HTTP 宿主端出去的一份产物。

| 路径 | 含义 |
|---|---|
| `{data}/gproxy.db` | 实例 |
| `{data}/gproxy.toml` | 可选；`AppConfig` 认得的任何字段 |
| `{data}/files/` | 发布出去的内容与下载的词表 |
| `{data}/secrets.json` | 每个密钥落在哪个存储——不含任何密钥材料 |
| `{data}/gateway-key` | `0600`，仅当没有 keychain 时存在 |

## 密钥

| 密钥 | 有 keychain | 没有 keychain |
|---|---|---|
| 主密钥 | 首启铸 32 字节 | **根本没有主密钥**，并给出 `gproxy` 已有的那条警告 |
| 网关 key | bootstrap 铸出来的那把 | 数据库旁边一个 `0600` 文件 |

keychain 走 [`keyring`](https://docs.rs/keyring)：Linux 上是 Secret Service，
macOS 上是 Keychain，Windows 上是凭据管理器。

主密钥**不**退回文件。一把躺在它所保护的数据库旁边的钥匙不叫加密，只是通往同
一份明文的一条更长的路。没有 keychain 时，实例的行为就跟服务器没设
`GPROXY_MASTER_KEY` 完全一样——而且用同样的措辞说出来，因为
`gproxy::rotate::PLAINTEXT_SECRETS` 是两个宿主共用的一句话，不同的只是补救
办法。`desktop_instance_status` 会报 `secretsAreSealed: false`，窗口据此把这
件事放在人看得见的地方。

网关 key 会退回文件。它是另一类密钥：它授权的是使用**这个实例**，而不是某人的
上游账号；没有它数据面根本跑不起来；而另一条路是让一台没有 keyring 守护进程的
机器上的应用干脆启动不了。

**keychain 弄丢了它本该持有的条目，是拒绝启动，而不是重新铸一把。**
`secrets.json` 记下主密钥落在哪个存储；没有这条记录，一个被清空的登录钥匙串看
起来就跟首启一样，于是会铸出一把**新的**主密钥，而数据库里已有的每条凭证都会
悄无声息地变成打不开的。拒绝启动时会指名要恢复哪个条目。

## IPC 命令表

261 条命令，由 [`src/ipc/table.rs`](src/ipc/table.rs) 里的一份声明生成：

```
admin     91      gproxy_app::Operations —— 身份家族
manage   137      gproxy.manage()        —— 引擎的配置家族
portal    14      gproxy_app::Portal     —— 终端用户自己的面
query     11      gproxy.query()         —— 用量、配额、日志
upstream   5      gproxy.login()         —— 三种上游登录
desktop    3      壳自己的状态
```

命令名是 `{surface}_{family}_{method}`——`admin_users_list` 就是
`Operations::users().list(..)`。这张表以**操作**为准，**绝不以 HTTP 宿主的路由
为准**：两个面是同一层产品之上的兄弟，`gproxy-host-axum` 重排路由不构成这边的
改动。

```
admin users [.users()] {
    list(query: app::ListQuery);
    get[id];
    create(write: app::UserWrite);
    update[id](patch: app::UserPatch);
    delete[id];
    batch(items: Vec<app::BatchItem<app::UserWrite, app::UserPatch>>);
    set_password[id, new_password];
}
```

方括号里的名字变成 `String` 参数、以 `&str` 传进去；圆括号里的反序列化后按值
传。末尾的 `@manual { … }` 块列出十六个语法**故意**不覆盖的方法——切片参数、
不能交给 webview 的时钟、不返回 `Result` 的同步访问器——它们是普通函数，登记在
同一份清单里。

一条命令只做三件事：解包、调一个方法、渲染。不做校验、不做授权、不按返回值
分支，因为每条规则在这条线以下都已经有主，在这里再给一份意见只会变成"IPC 上
成立、HTTP 上不成立"的规则。

不想展开宏也能读这张表：

```sh
cargo test -p gproxy-host-tauri --test table -- --ignored --nocapture
```

### IPC 侧的认证

没有，这是设计。消息能到这儿，只因为本进程自己的 webview 发了它，通道本身就是
证明；再加一道 key 校验等于让应用向自己证明自己。`Desktop::caller` 就是本机
管理员，`portal_*` 命令按构造被收窄到他，跟浏览器里登录过的用户完全一样。

桌面管理员有一个**谁都不知道**的密码——bootstrap 会生成一个，而这个壳从不把它
打印出来，因为 `Report::announce` 是命令行的呈现方式，`Desktop::start` 不调它。
这里没人需要登录，回环 socket 上那道浏览器的门本来也是关着的；一道关着的门后面
放一个猜不到的秘密，比根本不放要好。真想把这道门打开，就调
`admin_users_set_password` 自己定一个。

### `transport.ts` 这道缝

P13/P14 给两个宿主建同一份控制台。类型两边都由 ts-rs 从同一批 DTO 生成；不同的
只是怎么发起调用，而这个不同应该只住在一个文件里：

```ts
// console/src/lib/transport.ts
export const transport = "__TAURI_INTERNALS__" in window
  ? { call: (op: string, args?: object) => invoke(op, args) }
  : { call: (op: string, args?: object) => fetchJson(routeOf(op), args) };
```

两边稳定的东西是操作名：IPC 上它是命令名，HTTP 上它是服务器为同一个家族同一个
方法挂的路径。控制台里除此之外的任何地方都不该知道自己跑在哪个宿主上。控制台
的构建产物放 `ui/`；在那之前那里只有一个占位页面。

## 本 crate 明确不做的事

自动更新、开机自启、系统托盘、移动端。这些每一件都是关于软件怎么**分发**的决
定，而不是关于它做什么，而且都是 v3 的打包遗留关切。等真有人在用这个桌面壳并
且需要它们再加；先加只会让一个还没有用户的应用背上一条更新通道。

它也不构建控制台，不绑 OAuth issuer：`/v1/oauth/*` 是这个实例为**下游**客户端
跑的授权服务器，由那些会把浏览器重定向过去的程序经 HTTP 访问。在签发它的这个
应用自己的 IPC 桥上调 `token`，没有意义。

## 复用而非重写

[`gproxy`](../gproxy)——命令行的库那一半——已经会把 `AppConfig` 变成一个运行中的
实例（`instance::open`）、轮换主密钥、建第一个管理员（`bootstrap::ensure_admin`）
并装好日志 subscriber。这四件事都是在这里直接调的。搬不过来的是**配置分层**：
clap 压环境、环境压 `.env` 是运维的接口，而窗口三样都没有。

## 构建

Tauri 在 Linux 上的依赖是 `webkit2gtk-4.1`、`gtk+-3.0` 与 `libsoup-3.0`。正因
如此，本 crate **不在工作区的 `default-members` 里**：裸 `cargo check` 会跳过
它，`cargo check --workspace` 会构建它。根 `Cargo.toml` 里的注释记了这个决定背
后的实测数据。

```sh
cargo check   -p gproxy-host-tauri
cargo clippy  -p gproxy-host-tauri --all-targets --all-features -- -D warnings
cargo test    -p gproxy-host-tauri
cargo run     -p gproxy-host-tauri --bin gproxy-desktop
```

测试从不碰真的凭据存储：`secrets::MemoryStore` 与 `secrets::UnavailableStore`
分别扮演能用的 keychain 和缺失的 keychain；`tests/assembly.rs` 在临时目录上启
一个完整实例，并经 Tauri 的 `MockRuntime` 走真正的命令表派发。
