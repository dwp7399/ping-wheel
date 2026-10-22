# 竞品与可借鉴的项目

定期回来看看这些项目有什么新东西，挑能借鉴的点记到「可以学的」里。最近一次整理：2026-10-22。

## 轮盘 / 信号类

| 项目 | 形态 | 值得学的 |
|---|---|---|
| [darrenprx/lolPing](https://github.com/darrenprx/lolPing) | Electron + C++ 钩子，Win / macOS / Linux，MIT，约 1.7k star | 仿 LOL 信号轮盘的手感；C++ 小进程接管全局鼠标键盘并吞掉事件；每块屏幕一个透明穿透窗口；局域网 / Nostr 房间共享信号 |
| [norvancc/super-ping](https://github.com/norvancc/super-ping) | Swift 原生 macOS，基于 lolPing 重写 | 每个方向可配动作（截图问 Codex、保存截图、跑快捷指令 / 脚本）；截图保留信号标记；记录最前窗口和浏览器链接 |
| [C6oisini/lolPing](https://github.com/C6oisini/lolPing) | lolPing 分支 | 轮盘上的「问 AI」：截图后先框选区域再提问，回答流式 Markdown，思考过程折叠 |
| [Kando](https://kando.menu/usage/) | 跨平台开源饼菜单 | 四种选择模式（点击、手势、Turbo、悬停）；子菜单；菜单编辑器 |
| [CopyWheel](https://trendshift.io/repositories/223249) | Swift 原生，MIT | 光标处弹出的饼菜单，放文本片段 |
| [Loop](https://github.com/mrkai77/loop) | macOS 窗口管理 | 用径向菜单 + 鼠标方向摆放窗口，交互很顺 |
| Radial / Pieoneer / Ringit（灵环） | 闭源付费 | 商业化的饼菜单启动器；Radial 5.0 加了 Claude 驱动的文本 AI 动作 |

## 划词 / 截图问 AI 类

| 项目 | 形态 | 值得学的 |
|---|---|---|
| [Raycast AI Commands](https://manual.raycast.com/ai/ai-commands) | 启动器内置 AI 指令 | 预设指令清单（改写、纠错、用简单的话解释、找 bug、总结网页、问网页）；[presets.ray.so](https://presets.ray.so) 分享预设 |
| [划词翻译 hcfy.ai](https://hcfy.ai/en/docs/guides/summary) | 浏览器扩展 | 多个翻译 / AI 服务结果并排对比；截图翻译；PDF 划词 |
| [Select2AI](https://greasyfork.org/fil/scripts/553004-select2ai) | 油猴脚本 | 选中文字发给 AI，自定义提示词，用链接分享提示词 |
| [Screenshot To AI](https://chromeboard.com/extension/screenshot-to-ai-jhlolbkodlggdbhijicplfhcpigdjoij) | Chrome 扩展 | 框选截图 + 自定义提示词，一键发给多个 AI |
| [suhaasteja/tooltip](https://github.com/suhaasteja/tooltip) | macOS 服务菜单 | 选中文字，在光标处的悬浮面板给出回答 |
| Bob、PopClip、沉浸式翻译、Kimi 划词 | 闭源 | 划词翻译 / 动作条的交互细节和插件生态 |

## 设计参考

- [Open Design · Duolingo 设计系统](https://open-design.ai/plugins/design-system-duolingo/)：饱和色、3D 底边（按下时下沉）、圆润几何。轮盘的「活泼」风格参考了它
- [shadcn.io · Duolingo DESIGN.md](https://www.shadcn.io/design/duolingo/raw)：同一风格的设计令牌
- [GitHub Primer](https://primer.style)：管理台的风格参考

## 可以学的（待办）

- [ ] Kando 的 Turbo / 悬停模式：作为「点击执行」之外的可选手势
- [ ] lolPing 的多屏透明窗口与吞事件方案：Windows 版的输入层可以参考
- [ ] C6oisini/lolPing 的框选截图：「?」可选先框选区域
- [ ] Raycast 的预设分享：「?」的自定义目的、插件可以用链接分享
- [ ] hcfy 的多结果对比：翻译目的下可并排展示两个模型的结果
