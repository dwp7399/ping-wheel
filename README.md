# Ping Wheel

随时唤出的桌面信号轮盘（Tauri 2，先做 macOS，同时打包 Windows）。

**下载**：https://dwp7399.github.io/ping-wheel/

按 **⌘⇧C** 在光标处唤出轮盘，鼠标指向某一项会高亮，点左键执行；右键、Esc 或再按一次 ⌘⇧C 取消。轮盘打开期间的点击会被拦截，不会点到下面的应用。

每一项有固定方位（16 个方位可选，最多 12 项），选中范围按「离哪项最近」划分：在左上加一项，只会切分左和上之间的范围，其他方向不变。中心固定为菜单。

| 动作 | 说明 | 状态 |
|---|---|---|
| 菜单（中心） | 打开主窗口，编辑轮盘 | ✅ |
| 打开 | 打开链接、文件或应用 | ✅ |
| 问号 | 截图并让 LLM 解释，弹出对话框 | 开发中 |
| 标记 | 记录当前场景（链接、文件、窗口），可一键回去 | 开发中 |
| 收藏 | 截图后由 LLM 自动归类，同一来源放在一起 | 开发中 |
| 对话 | 新开一个对话 | 开发中 |

## 开发

```bash
pnpm install
pnpm tauri dev
```

App 只在菜单栏显示图标，不出现在 Dock 里。首次运行需要在「系统设置 → 隐私与安全性 → 辅助功能」里授权，用于跟踪鼠标和拦截轮盘上的点击；没有授权时会退回轮询方式。

在浏览器打开 `http://localhost:1420` 可以单独调轮盘界面：`?n=8` 指定数量，`?view=editor` 是设置页，`?view=toast` 是执行提示。

## 发布

推送 `v*` 标签后，GitHub Actions 会打包 macOS（Apple 芯片 / Intel）和 Windows 安装包并发布到 Releases；`site/` 是下载页，改动后自动部署到 GitHub Pages。

## 结构

- `src-tauri/src/wheel.rs`：快捷键、光标跟踪、方向判定
- `src-tauri/src/input_macos.rs`：macOS 鼠标事件监听与点击拦截
- `src-tauri/src/config.rs`：轮盘配置读写
- `src-tauri/src/actions.rs`：各动作与执行提示
- `src/Wheel.tsx`：轮盘界面；`src/WheelEditor.tsx`：设置；`src/Toast.tsx`：执行提示
- `site/index.html`：下载页
