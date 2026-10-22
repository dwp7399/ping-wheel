# Ping Wheel

随时唤出的桌面信号轮盘（Tauri 2，先做 macOS，同时打包 Windows）。

**下载**：https://dwp7399.github.io/ping-wheel/

按 **⌘⇧C** 在光标处唤出轮盘，鼠标指向某一项会高亮，点左键执行；右键、Esc 或再按一次 ⌘⇧C 取消。轮盘打开期间的点击会被拦截，不会点到下面的应用。

每一项有固定方位（16 个方位可选，最多 12 项），选中范围按「离哪项最近」划分：在左上加一项，只会切分左和上之间的范围，其他方向不变。中心固定为菜单。

| 动作 | 说明 |
|---|---|
| 菜单（中心） | 主窗口：标记、收藏、轮盘、插件、账号 |
| ? | 截取整块屏幕并圈出标记位置，连同标记处 / 附近 / 选中的文字和网页正文一起交给模型，判断你想知道什么；结果显示在悬浮面板里，可追问 |
| 对话 | 打开悬浮对话面板 |
| 标记 | 记下当前的网页链接 / 文件 / 应用和截图，之后一键回到当时的场景（应用没开会自动启动） |
| 收藏 | 截图收藏，自动生成标题、分类、标签和摘要；同一链接或文件的收藏归在一起 |
| 打开 | 打开链接、文件或应用 |
| 插件 | 运行自定义插件的动作，见 [插件开发](docs/plugins.md) |

悬浮面板不常驻：点别处或按 Esc 就关闭，关闭即结束，不保存对话；点图钉可以把它钉在桌面上。面板只显示「思考中」和回答，不显示截图和模型的思考过程。

首次使用需要授权：辅助功能（跟踪鼠标、读界面文字）、屏幕录制（截图）、自动化（读浏览器链接和网页内容）。想让「?」读到网页正文，还需要在 Chrome 菜单「显示 → 开发者 → 允许 Apple 事件中的 JavaScript」（Safari 在「开发」菜单）里开启。

### 语言

默认按系统时区自动选择语言（简体中文、繁體中文、English、日本語、한국어、Español、Português、Français、Deutsch、Русский、Tiếng Việt、Bahasa Indonesia），对不上时用英文；可在「账号 → 偏好」里手动指定。模型始终用所选语言回答；界面目前提供中文和英文，其他语言的界面暂用英文。界面文案以中文原文为键，对照表在 `i18n/`，前端和 Rust 共用。

### 模型能力与订阅

客户端不直连任何模型服务，也不记录模型调用。问号、对话、收藏归类和插件的模型调用都经过 Ping Wheel 服务（`src-tauri/src/cloud.rs`），各功能的系统提示词、模型选择和计费都在服务端。在主窗口「账号」页用邮箱验证码登录（没有账号会自动注册）。

- 高级功能：解锁自定义插件
- 模型能力：按周期（31 天）订阅，服务端按实际用量从额度里扣除

## 开发

```bash
pnpm install
pnpm tauri dev
```

App 只在菜单栏显示图标，不出现在 Dock 里。首次运行需要在「系统设置 → 隐私与安全性 → 辅助功能」里授权，用于跟踪鼠标和拦截轮盘上的点击；没有授权时会退回轮询方式。

在浏览器打开 `http://localhost:1420` 可以单独调轮盘界面：`?n=8` 指定数量，`?view=editor` 是设置页，`?view=toast` 是执行提示，`?view=main` / `?view=panel` 用假数据预览主窗口和悬浮面板。

## 发布

推送 `v*` 标签后，GitHub Actions 会打包 macOS（Apple 芯片 / Intel）和 Windows 安装包并发布到 Releases；`site/` 是下载页，改动后自动部署到 GitHub Pages。

## 结构

- `src-tauri/src/wheel.rs`：快捷键、光标跟踪、方向判定
- `src-tauri/src/input_macos.rs`：macOS 鼠标事件监听与点击拦截
- `src-tauri/src/config.rs`：轮盘配置读写
- `src-tauri/src/actions.rs`：动作分发与执行提示
- `src-tauri/src/features.rs`：问号、对话、标记、收藏
- `src-tauri/src/cloud.rs`：服务端能力客户端（所有模型调用都经过这里）
- `src-tauri/src/panel.rs`：悬浮面板
- `src-tauri/src/plugins.rs`：插件安装、卸载与运行
- `src-tauri/src/scene.rs`：场景采集（应用、窗口、链接、文件、标记处文字、网页正文）
- `src-tauri/src/shot.rs`：截图、标记绘制与裁剪
- `src-tauri/src/store.rs`：SQLite 存储（标记、收藏）
- `src-tauri/src/settings.rs`：账号设置
- `src/Wheel.tsx`：轮盘界面；`src/WheelEditor.tsx`：设置；`src/Toast.tsx`：执行提示
- `src/Panel.tsx`：悬浮面板；`src/PluginRunner.ts`：插件运行器；`src/pages.tsx`：标记、收藏、插件、账号页
- `examples/plugins/`：示例插件；`scripts/pack-plugin.sh`：打包插件
- `site/index.html`：下载页
