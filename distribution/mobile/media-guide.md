# 上架截图与演示视频

## 已制作的内容

运行 `scripts/mobile/capture-listing.py --video` 后，素材位于
`dist/mobile/listing-materials/`：

- 英文、简体中文、繁体中文，各有四张手机布局截图和四张桌面布局截图。
- 手机布局为 1080 × 1920 PNG；桌面布局为 1920 × 1080 PNG。
- 场景依次为用量概览、供应商凭据、模型路由、供应商模型。
- 英文、简体中文各有一段带字幕的 MP4 功能演示，以及单独的字幕文件。
- `capture-manifest.json` 记录源码提交、采集方式、尺寸和页面错误检查结果。

这些图片来自当前 v4 控制台的真实前端，在隔离的 Chrome 会话中使用演示接口
数据生成。账号、凭据、模型名称、用量和费用均为示例，不含真实密钥、私人请求
或客户资料，也不是性能测量。原素材包 `dist/store/listing-materials/README.txt` 记录了相同的采集方式；旧素材是
v3 界面，未直接复用其中的图片。

这些是**通用控制台的界面素材候选**，不是安卓或鸿蒙设备截屏。提交到移动商店
前，应与最终安装包的界面逐项对照；不能据此声称已经完成安卓或鸿蒙设备测试。
它们没有伪造手机外框、系统通知栏或平台专属能力。

## 截图说明

| 文件 | 简体中文说明 | 繁體中文說明 | English caption |
| --- | --- | --- | --- |
| `01-overview.png` | 查看用量、费用趋势与预算。画面使用演示数据。 | 查看用量、費用趨勢與預算。畫面使用示範資料。 | Review usage, cost trends and budgets with demonstration data. |
| `02-providers.png` | 按供应商管理凭据，控制启用状态。 | 依供應商管理憑證，控制啟用狀態。 | Manage credentials and enabled state for each provider. |
| `03-model-routing.png` | 为统一模型名配置路由策略与尝试次数。 | 為統一模型名稱設定路由策略與嘗試次數。 | Configure routing strategies and attempt limits for a public model name. |
| `04-models.png` | 管理供应商提供的上游模型。 | 管理供應商提供的上游模型。 | Manage the upstream models offered by each provider. |

功能演示依次展示概览、供应商与凭据、模型目录、路由目标和权重，以及开源项目
信息。视频字幕说明了演示数据及采集方式。它可以作为产品功能介绍材料，**不能
替代 Google Play 前台服务权限申报要求的真实安卓录像**。

## 安卓后台服务专项审核录像

这段录像仍需在安卓启动问题解决后，用实际安装包录制。不要将浏览器功能视频
填成这项权限的证明。建议控制在一至两分钟，完整保留以下真实操作：

1. 打开应用，首次使用时阅读隐私说明，完成本机网关设置。
2. 明确启动网关，显示运行中的前台通知和本机服务地址。
3. 在测试客户端向该地址发送一条受访问密钥保护的请求。仅使用受限测试账号，
   不在录像中显示密钥或生产数据。
4. 返回系统桌面，展示网关继续运行的通知；必要时再发一次请求证明实际用途。
5. 展开通知，点击“停止”，展示通知消失、网关停止。
6. 若本版本申报开机启动或电池豁免，另行说明真实用户授权路径。

测试步骤没有通过就保留为待办，不通过剪辑把失败改成成功。原生鸿蒙的安装、
启动和后台能力也应另行录制，不能使用安卓或浏览器画面冒充。

## 重新采集

使用独立浏览器会话，不需要连接真实网关：

```sh
# 一个终端，明确避免访问日常运行的后台实例
GPROXY_DEV_BACKEND=http://127.0.0.1:1 pnpm --dir console dev --host 127.0.0.1 --port 5186 --strictPort

# 另一个终端，使用装有这些工具的 Python 环境
python -m pip install playwright imageio-ffmpeg
python -m playwright install ffmpeg
python scripts/mobile/capture-listing.py --video
```

脚本使用 `/usr/bin/google-chrome`，在独立会话中拦截演示接口，并阻止外部请求。
遇到未覆盖的接口或页面错误会停止，不生成看似正常的错误页截图。视频编码和
字幕渲染需要 FFmpeg 支持字幕滤镜及已安装的 Noto Sans CJK 字体。普通网站浏览
和这些素材的生成都不证明安装包的原生运行情况。

首发文案见 [first-release-notes.md](first-release-notes.md)，其中的商店版本尚未
发布；正式采用其他版本时，应同步修改版本号及更新说明文件名。
