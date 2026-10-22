# Ping Wheel

随时唤出的桌面信号轮盘（Tauri 2，先做 macOS，同时打包 Windows）。

**下载**：https://dwp7399.github.io/ping-wheel/

按 **⌘⇧C** 在光标处唤出轮盘，鼠标指向某一项会高亮，点左键执行；右键、Esc 或再按一次 ⌘⇧C 取消。轮盘打开期间的点击会被拦截，不会点到下面的应用。

每一项有固定方位（16 个方位可选，最多 12 项），选中范围按「离哪项最近」划分：在左上加一项，只会切分左和上之间的范围，其他方向不变。中心固定为菜单。

| 动作 | 说明 |
|---|---|
| 菜单（中心） | 主窗口：标记、收藏、轮盘设置、模型设置 |
| 问号 | 截取光标所在屏幕，连同当前应用、窗口、链接一起交给模型解释，在对话窗口里流式回答，可继续追问 |
| 对话 | 打开对话窗口，新开一个对话 |
| 标记 | 记下当前的网页链接 / 文件 / 应用和截图，之后在「标记」里一键回到当时的场景 |
| 收藏 | 截图收藏，模型自动生成标题、分类、标签和摘要；同一链接或文件的收藏归在一起 |
| 打开 | 打开链接、文件或应用 |

首次使用需要授权：辅助功能（跟踪鼠标、读窗口信息）、屏幕录制（截图）、自动化（读浏览器当前链接）。

### 模型

所有模型调用都经过 `src-tauri/src/llm.rs` 这一个入口（OpenAI 兼容的 `/chat/completions`，流式），每次调用都会记日志并写入本地 `llm_calls` 表，以后切换网关、计量、限流都只改这里。默认对接火山方舟 Agent Plan（`https://ark.cn-beijing.volces.com/api/plan/v3`，模型 `glm-5.3-flash`），在主窗口「模型」页填写 Key。

## 开发

```bash
pnpm install
pnpm tauri dev
```

App 只在菜单栏显示图标，不出现在 Dock 里。首次运行需要在「系统设置 → 隐私与安全性 → 辅助功能」里授权，用于跟踪鼠标和拦截轮盘上的点击；没有授权时会退回轮询方式。

在浏览器打开 `http://localhost:1420` 可以单独调轮盘界面：`?n=8` 指定数量，`?view=editor` 是设置页，`?view=toast` 是执行提示，`?view=main` / `?view=chat` 用假数据预览主窗口和对话窗口。

## 发布

推送 `v*` 标签后，GitHub Actions 会打包 macOS（Apple 芯片 / Intel）和 Windows 安装包并发布到 Releases；`site/` 是下载页，改动后自动部署到 GitHub Pages。

## 结构

- `src-tauri/src/wheel.rs`：快捷键、光标跟踪、方向判定
- `src-tauri/src/input_macos.rs`：macOS 鼠标事件监听与点击拦截
- `src-tauri/src/config.rs`：轮盘配置读写
- `src-tauri/src/actions.rs`：动作分发与执行提示
- `src-tauri/src/features.rs`：问号、对话、标记、收藏
- `src-tauri/src/llm.rs`：统一的模型调用入口
- `src-tauri/src/scene.rs`：场景采集（应用、窗口、链接、文件）与截图
- `src-tauri/src/store.rs`：SQLite 存储
- `src-tauri/src/settings.rs`：模型设置
- `src/Wheel.tsx`：轮盘界面；`src/WheelEditor.tsx`：设置；`src/Toast.tsx`：执行提示
- `src/ChatWindow.tsx`：对话窗口；`src/pages.tsx`：标记、收藏、模型设置页
- `site/index.html`：下载页
