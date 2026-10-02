# GPROXY Android 应用

[English](ANDROID.md) | 简体中文

和桌面同一个壳，跑在手机上。同一个 `App`、同一张 IPC 命令表、同一个控制台、
同一套生成类型——外加一个前台服务把进程撑住，因为在 Android 上这是让网关持续
运行的唯一办法。

```
         窗口                        本机上的某个 App
          │                                 │
      Tauri IPC                    HTTP，127.0.0.1:8787
          │                                 │
  ipc::table（261 条命令）     gproxy-host-axum，只挂数据面
          └──────────────┬──────────────────┘
                  同一个 gproxy_app::App
              一份 Gproxy · 一份快照 · 一份 cache
                          │
                 GproxyService，前台服务
              进程还活着的唯一原因就是它
```

## 回环数据面才是意义所在

本机上的 App 指向 `http://127.0.0.1:8787`，连到的就是这个进程。手机能当网关
用，靠的就是这一条。

**它照样要 key**，和桌面一模一样，理由也一模一样：设备上任何进程都能连回环，
不鉴权的数据面等于让它们随便花用户的上游额度。同一套 authenticator、同一套准
入、同一个 `401`。`/admin/api` 与 `/portal/api` 在这里也照样回 `404`——管理面
是窗口的，走 IPC。

## Tauri 给了什么，以及它没给的三件事

Tauri v2 支持 Android，脚手架、WebView、IPC 桥和打包都给了。但手机上一个网关
真正需要的三种行为，它一个都没给。这三件事在 v3 手写的 Java 里全都有，这里是
**移植**，不是重新发明。

| | v3，Java | v4，`gen/android/` 里的 Kotlin |
|---|---|---|
| 前台服务 | `scripts/android/GproxyService.java.in` | `GproxyService.kt` |
| 开机接收器 | `scripts/android/GproxyBootReceiver.java.in` | `GproxyBootReceiver.kt` |
| 应用内更新 | `GproxyUpdateActivity.java.in`、`GproxyUpdateProvider.java.in` | `GproxyUpdateActivity.kt`、`GproxyUpdateProvider.kt` |

### 前台服务不再监管任何东西

这是与 v3 唯一实质的差别，而它改变了其余一切的形状。

v3 的服务把 `gproxy.bin` 从 APK 的 assets 里拷出来，设好 `LD_LIBRARY_PATH`，
以**子进程**方式拉起，然后轮询 `http://127.0.0.1:8787/admin` 直到它应答，再把
子进程的 stdout 抽进一个环形缓冲。App 和网关是两个进程，服务的职责是监管另一
个。

现在引擎被编进 `libgproxy_host_tauri.so`，数据面就在 App 自己的进程里监听。
没有子进程要管，没有 asset 要解包，没有健康轮询要写，也不存在两个进程对「打开
了哪个数据库」产生分歧的可能。服务只为一件事存在：**阻止 Android 杀掉引擎所在
的这个进程**。前台没有任何东西的 App，在用户切走后几秒内就会被冻结、然后被
杀。

有两个后果值得知道：

**Stop 会结束进程。** 实例每进程只装配一次，而 Tauri 已经把它作为 managed
state 交给了 261 条命令；不存在一种诚实的「停止」能让这些命令握着一个已关停的
实例。Stop 关掉 socket、停掉后台同步，然后结束进程。下一次启动是冷启动——这也
是唯一一种行为与首次启动完全一致的「重启」。

**服务类型是 `specialUse`，不是 `dataSync`。** 从 Android 14 起前台服务必须声
明类型；从 Android 15 起 `dataSync` 服务被限制在 24 小时内最多 6 小时。一个跑
满六小时就不再应答的网关，和一个切走 App 就死的网关是同一个 bug。`specialUse`
没有这个限制，并带一段声明用途的文案（`R.string.gproxy_special_use_subtype`）。
`dataSync` 也一并声明，是给那些认识「类型」但不认识 `specialUse` 的版本用的。

### 开机接收器

`BOOT_COMPLETED` 需要 `RECEIVE_BOOT_COMPLETED`；没有这个权限，接收器根本不会
被调用，而且是**静默**不调用。`MY_PACKAGE_REPLACED` 也一并过滤，重要性不亚于
前者：没有它，每次更新都会变成一次停服，直到有人碰巧打开 App。

接收器只负责启动服务然后返回。它绝不碰引擎——广播接收器在主线程上只有约十秒，
超时 Android 就判定它卡死，而打开一个冷数据库可能比这更久。

### 应用内更新

清单里的 `REQUEST_INSTALL_PACKAGES` 让安装 Intent 合法。而「安装未知应用」那个
按应用开关是另一回事，属于用户授权，`GproxyUpdateActivity` 在真正需要的那一刻
用 `ACTION_MANAGE_UNKNOWN_APP_SOURCES` 去申请；没有它就直接发安装 Intent 会被
静默拒绝。

`GproxyUpdateProvider` 只暴露一条只读的 `content://` 路径：App 私有存储里的文
件系统安装器读不到，而从 Android 7 起,指向它的 `file://` URI 是
`FileUriExposedException` 而不是一次安装。

前台通知的 **Update** 按钮打开更新界面。工作线程复用 `gproxy update` 的 Rust
校验路径：ed25519 签名、通道、数据兼容版本、长度和 SHA-256。校验后先落盘
`gproxy-update.apk`，再写 `install-apk.pending`，最后交给 Android 安装器确认。
不会按计划自动安装。

清单使用 `<target-triple>-tauri-apk` 条目，与旧版 `-apk` 分开，后者的应用 ID
不同。将 `gproxy-tauri-android-aarch64.apk`（或 `gproxy-tauri-android-x86_64.apk`）
及其 `.sha256` 放在原生产物旁，`scripts/build-update-manifest.sh` 会将它纳入签名
清单。构建时提供 `GPROXY_UPDATE_PUBKEY`；滚动构建还需
`GPROXY_BUILD_CHANNEL=dev` 和 `GPROXY_BUILD_HASH`。APK 必须使用与已安装应用
相同的 Android 签名密钥。Release 工作流构建两种应用架构，并将签名后的 APK 交给清单任务；
清单缺少该产物时明确报不可用，不会退回旧版壳 APK。

## 为什么网关是进程，而不是窗口

这套安排之所以合法，依赖 Tauri 生成代码里的一个事实，所以这里带出处地写明：
`gen/android/app/src/main/java/com/leenhawk/gproxy/app/generated/WryActivity.kt`
是从 **`ProcessLifecycleOwner`** 而不是 activity 里调用 `Rust.create()` 的——
也就是真正运行 `gproxy_host_tauri::start` 的那一次调用：

```kotlin
object WryLifecycleObserver : DefaultLifecycleObserver {
    override fun onCreate(owner: LifecycleOwner) {
        Rust.create()
        Rust.wryCreate()
    }
}
```

因此 Rust 的 main **每进程只跑一次**。前台服务在窗口销毁后继续把进程撑着，是
一个被支持的状态；之后新建的 activity 会通过 `onActivityCreate` 重新挂上去，
而不是启第二个引擎。这也是本 crate 里没有任何重入保护的原因：加了反而是在防一
件框架已经防住的事。

`engine::ensure_started` 也是因此而存在。窗口不是唯一入口——开机接收器没有窗口
——所以实例是一个「谁先问谁装配」的进程级全局；runtime 从 `run()` 里搬出来也是
同一个理由：`run()` 返回时被 drop 掉的 runtime，会把监听器和后台同步一起带走。

## 手机上的机密

`keyring` 没有 Android 后端。每次调用都返回 `Invalid("platform", …)`，而
`secrets` 本来就把这读作「这台机器没有 keychain」——所以 `SecretStore` 这层抽象
一行都不用改，行为就是已经写在文档里的「桌面上没有 keyring daemon」那一种：

| 机密 | 在 Android 上 |
|---|---|
| master key | **不生成**；上游凭据以明文存储 |
| gateway key | 数据目录下的 `0600` 文件 |

`desktop_instance_status` 会报 `secretsAreSealed: false`，服务的通知里会写出
来，`gproxy::rotate::PLAINTEXT_SECRETS` 也会用与服务端完全相同的措辞记进日志。

Android 的按应用 UID 让这个问题比桌面上同样的情况要轻——数据目录是**别的 App**
读不到，而不只是别的用户读不到——但它不等于没有问题：`adb`、备份代理和 root 都
读得到。**通过第二个 `SecretStore` 实现把 master key 包进 Android Keystore，是
已经点名的后续工作**；而这个 trait 的存在，正是为了让那件事是加一个新文件，而
不是改八处调用点。

## 它需要什么

| | 这里用的版本 |
|---|---|
| Android SDK | platform `android-36`，build-tools 36.1.0 |
| NDK | 30.0.15729638（`NDK_HOME` 必须指向它） |
| JDK | 21 |
| Gradle | 8.14.3，由 `gen/android/` 里的 wrapper 拉取 |
| Rust targets | `aarch64-linux-android` 及另外三个 ABI |
| Tauri CLI | 2.11.5，在 `package.json` 里钉死 |

Tauri CLI 是**本 crate 的 dev 依赖**，不是全局工具：

```sh
cd crates/gproxy-host-tauri
pnpm install
```

`gen/android/buildSrc/.../BuildTask.kt` 会回调
`node_modules/@tauri-apps/cli/tauri.js`，因此 Gradle 构建和 `pnpm` 构建用的是
同一个版本的工具。**那个文件是打过补丁的**：生成器写的是 `listOf("tauri", …)`，
实际调用变成 `node tauri …` 并失败，因为 crate 里根本没有叫 `tauri` 的文件。
如果将来重跑 `tauri android init`，需要重新打这个补丁——补丁处有注释说明。

## 构建

```sh
cd crates/gproxy-host-tauri
export NDK_HOME=/path/to/Android/Sdk/ndk/30.0.15729638
export ANDROID_NDK_HOME="$NDK_HOME"
# NDK 30 requires a versioned target when bindgen reads its headers.
export BINDGEN_EXTRA_CLANG_ARGS_aarch64_linux_android=--target=aarch64-linux-android28
export BINDGEN_EXTRA_CLANG_ARGS_x86_64_linux_android=--target=x86_64-linux-android28
export BINDGEN_EXTRA_CLANG_ARGS_armv7_linux_androideabi=--target=armv7a-linux-androideabi28
export BINDGEN_EXTRA_CLANG_ARGS_i686_linux_android=--target=i686-linux-android28

pnpm android:build:arm64                          # release APK，只出 arm64
pnpm exec tauri android build --apk               # 四个 ABI 全出
pnpm exec tauri android build --apk --debug       # debug
```

这个引擎的 debug `.so` 带着约一个 GB 的符号；release profile
（`opt-level = "z"`、`lto = "fat"`、`strip = "symbols"`）把它压到约 43 MB。除非
要挂调试器，否则一律构建 release。

## 把 APK 读回来（v4.0.0 历史构建）

下面是标识更名前的真实输出，保留旧包名和 JNI 名称作为历史记录；当前构建使用
`com.leenhawk.gproxy.app`。

对 release 构建跑 `aapt dump badging`，略去九十多行翻译过的
`application-label-*`：

```
package: name='dev.gproxy.desktop' versionCode='4000000' versionName='4.0.0' platformBuildVersionName='16' platformBuildVersionCode='36' compileSdkVersion='36' compileSdkVersionCodename='16'
sdkVersion:'28'
targetSdkVersion:'36'
uses-permission: name='android.permission.INTERNET'
uses-permission: name='android.permission.FOREGROUND_SERVICE'
uses-permission: name='android.permission.FOREGROUND_SERVICE_SPECIAL_USE'
uses-permission: name='android.permission.FOREGROUND_SERVICE_DATA_SYNC'
uses-permission: name='android.permission.POST_NOTIFICATIONS'
uses-permission: name='android.permission.RECEIVE_BOOT_COMPLETED'
uses-permission: name='android.permission.REQUEST_INSTALL_PACKAGES'
uses-permission: name='android.permission.REQUEST_IGNORE_BATTERY_OPTIMIZATIONS'
uses-permission: name='dev.gproxy.desktop.DYNAMIC_RECEIVER_NOT_EXPORTED_PERMISSION'
application-label:'GPROXY'
application: label='GPROXY' icon='res/9w.png'
launchable-activity: name='dev.gproxy.desktop.MainActivity'  label='GPROXY' icon=''
leanback-launchable-activity: name='dev.gproxy.desktop.MainActivity'  label='GPROXY' icon='' banner=''
feature-group: label=''
  uses-feature-not-required: name='android.software.leanback'
  uses-feature: name='android.hardware.faketouch'
  uses-implied-feature: name='android.hardware.faketouch' reason='default feature for all apps'
main
other-activities
other-receivers
other-services
supports-screens: 'small' 'normal' 'large' 'xlarge'
supports-any-density: 'true'
native-code: 'arm64-v8a'
```

`aapt dump badging` 不会列出 service 和 receiver 的名字，所以组件来自
`aapt2 dump xmltree --file AndroidManifest.xml`：

```
E: service (line=99)
  android:name="dev.gproxy.desktop.GproxyService"
  android:exported=false
  android:stopWithTask=false
  android:foregroundServiceType=0x40000001
E: receiver (line=110)
  android:name="dev.gproxy.desktop.GproxyBootReceiver"
  android:exported=true
    E: action  android:name="android.intent.action.BOOT_COMPLETED"
    E: action  android:name="android.intent.action.MY_PACKAGE_REPLACED"
E: activity  android:name="dev.gproxy.desktop.GproxyUpdateActivity"  android:exported=false
E: provider  android:name="dev.gproxy.desktop.GproxyUpdateProvider"
  android:authorities="dev.gproxy.desktop.updates"  android:exported=false
```

`0x40000001` 即 `FOREGROUND_SERVICE_TYPE_SPECIAL_USE (0x40000000) |
FOREGROUND_SERVICE_TYPE_DATA_SYNC (0x1)`。

原生库，来自 `unzip -l`：

```
 42985744  1981-01-01 01:01   lib/arm64-v8a/libgproxy_host_tauri.so
```

APK 共 46,607,735 字节、919 个条目。只有 `arm64-v8a`，因为上面那次构建只要了
一个 ABI；`--apk` 不带 `--target` 会出四个。

release 构建会跑 R8，而 JNI 这条缝依赖类名和方法名在 R8 之后原样保留。来自
`mapping.txt`：

```
dev.gproxy.desktop.GproxyNative -> dev.gproxy.desktop.GproxyNative:
```

是恒等映射，所以 `Java_dev_gproxy_desktop_GproxyNative_nativeStart` 仍然能解析
到。`proguard-rules.pro` 把 keep 规则明写了出来，而不是依赖默认规则文件里有。

## **未经验证**的部分

这台机器没有连接设备，也没有 X server。**下面所有内容都没有被实际运行观察
过。** 上文的一切要么是源码，要么是从构建出的 APK 里读回来的输出。

- **这个应用从未被启动过。** 没在真机上，也没在模拟器上。窗口打开、控制台渲
  染、某条 IPC 命令返回、数据面在回环上应答——在 Android 上这些都没有被看到
  过。它们在桌面上由 `tests/assembly.rs` 跑同一份代码覆盖，那是旁证，不是证
  明。
- **前台服务从未发出过通知**；「进程能在切到后台后存活」是依据 Android 文档行
  为的推断，不是观察结果。
- **开机接收器从未被触发过。**
- **应用内更新尚未在设备上验证。** 下载和校验由本地签名清单测试覆盖；系统安装器、
  未知来源授权界面和替换已安装 APK，仍需要设备及使用相同密钥签名的版本验证。
- **此前本地检查的 release APK 未签名。** 没有 signing config 时 Gradle 产出的就是
  `app-universal-release-unsigned.apk`，它不能直接安装。debug APK 用本机
  debug key 签过。
- **此前记录的本地构建只覆盖 `arm64-v8a`。** 另外三个 ABI 已配置、Rust target 也已安装，但没有
  编译出任何 `armeabi-v7a`、`x86` 或 `x86_64` 的库。
- **JNI 函数里的 `catch_unwind` 在 release 构建中什么也抓不到。** 工作区的
  release profile 是 `panic = "abort"`。这个保护在 debug 构建里是真的——而 debug
  正是有人想弄明白「它为什么 panic 了」时会跑的那一个。
- **电池优化豁免对话框**只申请一次，效果完全取决于厂商。

## Release CI

`.github/workflows/release.yml` 的 `application` 矩阵构建 ARM64 与 x86_64 APK。
`scripts/package-tauri-release.sh` 在 Gradle 构建后使用 `zipalign` 与 `apksigner` 对齐、签名并
验证 APK。任务安装以下工具：

- 带 platform `android-36` 与 build-tools 36.1.0 的 **Android SDK**，并已接受
  许可；
- 钉死版本的 **NDK**——`NDK_HOME` 会被 Tauri CLI 读取，且 NDK 版本决定了 `.so`
  链接到的 libc 符号；
- 一个 **JDK 21**；
- 通过仓库内 wrapper 的 **Gradle**，并缓存 `~/.gradle`——没有缓存的首次构建会下
  载 Gradle 本身、Android Gradle Plugin 和一堆 AndroidX 依赖；
- 在 `crates/gproxy-host-tauri` 里跑 **`pnpm install`** 装那个钉死的 CLI；
- 当前任务选中的 **Rust Android target**，每个任务构建一个 ABI；
- **签名密钥**：`ANDROID_SIGNING_KEYSTORE_B64`、
  `ANDROID_SIGNING_KEYSTORE_PASSWORD`、`ANDROID_SIGNING_KEY_ALIAS`，以及可选的
  `ANDROID_SIGNING_KEY_PASSWORD`。缺少必需的密钥会让发布任务失败。

意外的瓶颈是磁盘：这个引擎的 debug `.so` 约一个 GB，四个 ABI 足以撑爆一台小
runner。

## v3 的 APK 机器还在

Android 相关的东西一样都没删。`scripts/android/`、
`scripts/package-android-apk.sh` 以及两个 `*-linux-android` 发布目标作为参考保留。CLI 发布产物现为 ZIP 和 Termux DEB，不再构建旧 wrapper APK。
它们是两回事：

- 那些 **Java 模板**是这次移植的源材料，也是「某处当初这么做是不是另有原因」时
  的参照；
- **`*-linux-android` 目标**是给 Termux 的——在 Android ABI 上跑的纯服务端二进
  制，从 shell 里启动——那是与「带窗口的 App」完全不同的另一个故事，不受这里任
  何改动影响。

当前图形版在安卓、鸿蒙和桌面平台统一使用 `com.leenhawk.gproxy.app`，
命令行版 **GPROXY CLI** 已经存在，其对应的项目命名为 `com.leenhawk.gproxy.cli`。
macOS CLI 新服务使用该标识，已有旧服务继续兼容。v3 旧封装包仍为
`io.github.leenhawk.gproxy`，早期 v4 包使用 `dev.gproxy.desktop`。

更名后的应用属于新的安装身份，不能直接覆盖旧移动端包；桌面默认数据目录和
钥匙串服务名也会改变。切换前请在旧应用中导出配置，再到新应用中导入。旧数据
和凭据不会被删除，也不会自动迁移。微软商店包身份是 Partner Center 分配的
独立标识，仍须使用微软分配的值，不等同于 Tauri 的应用标识。
