/**
 * 浏览器预览用的假数据：模拟 Tauri 命令和事件，方便不启动 App 也能调主窗口和对话窗口的界面。
 * 只在预览模式（非 Tauri 环境）下加载。
 */
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import { DEFAULT_CONFIG } from "./types";

// 1x1 灰色 PNG，充当截图
const PIXEL =
  "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mN8+P9/PQAJYwN4V0dWJwAAAABJRU5ErkJggg==";

const now = Date.now();
const marks = [
  {
    id: 2, created_at: now - 600_000, app_name: "Google Chrome", app_path: "/Applications/Google Chrome.app",
    window_title: "Rust 所有权与借用 - Rust 程序设计语言", url: "https://kaisery.github.io/trpl-zh-cn/ch04-00-understanding-ownership.html",
    doc_path: null, image_path: "/shot/1.png", note: "下周复习",
  },
  {
    id: 1, created_at: now - 86_400_000, app_name: "预览", app_path: "/System/Applications/Preview.app",
    window_title: "季度报告.pdf", url: null, doc_path: "/Users/ys/Documents/季度报告.pdf", image_path: null, note: "",
  },
];
const collections = [
  { id: 3, created_at: now - 300_000, image_path: "/shot/2.png", app_name: "Google Chrome", app_path: null, window_title: "Tauri 2 窗口管理",
    url: "https://v2.tauri.app/learn/window-customization/", doc_path: null, source_key: "https://v2.tauri.app/learn/window-customization",
    title: "Tauri 自定义窗口标题栏", category: "编程", tags: ["Tauri", "窗口"], summary: "介绍如何用 data-tauri-drag-region 实现自定义标题栏和拖动。", status: "done" },
  { id: 4, created_at: now - 200_000, image_path: "/shot/3.png", app_name: "Google Chrome", app_path: null, window_title: "Tauri 2 窗口管理",
    url: "https://v2.tauri.app/learn/window-customization/", doc_path: null, source_key: "https://v2.tauri.app/learn/window-customization",
    title: "透明窗口与阴影", category: "编程", tags: ["Tauri"], summary: "transparent 与 macOSPrivateApi 的配合。", status: "done" },
  { id: 5, created_at: now - 60_000, image_path: "/shot/4.png", app_name: "小红书", app_path: null, window_title: "周末露营装备清单",
    url: null, doc_path: null, source_key: "app:com.xingin:周末露营装备清单", title: "周末露营装备清单", category: "", tags: [], summary: "", status: "classifying" },
];
const answer =
  "这是 **Rust 的所有权转移（move）**：`let s2 = s1;` 之后 `s1` 不再可用。\n\n- `String` 的数据在堆上，赋值只复制指针，所有权交给 `s2`\n- 离开作用域时只会释放一次，避免重复释放\n- 想保留 `s1`，用 `s1.clone()` 深拷贝";

const plugins = [
  {
    manifest: {
      id: "com.pingwheel.translate", name: "截图翻译", version: "1.0.0", description: "把标记处的内容翻译成中文", author: "Ping Wheel",
      permissions: ["scene", "screenshot", "llm"], actions: [{ id: "translate", label: "翻译", icon: "translate" }],
    },
    permission_labels: ["读取当前应用、窗口、链接和标记处的文字", "截取当前屏幕", "调用通用模型接口（消耗你的模型额度）"],
  },
];

export function installPreviewMocks(label: string) {
  mockWindows(label, "main", "wheel", "toast", "panel-1");
  const sp = new URLSearchParams(location.search);
  mockIPC(
    (cmd, args) => {
      const a = (args ?? {}) as Record<string, unknown>;
      switch (cmd) {
        case "get_config":
          return DEFAULT_CONFIG;
        case "marks_list":
          return marks;
        case "collections_list":
          return collections;
        case "image_data_url":
          return PIXEL;
        case "account_get":
          return { server_url: "http://127.0.0.1:8787", has_token: true, token_hint: "…167d" };
        case "account_me":
          return {
            user: { id: 1, name: "测试用户", email: "test@example.com" },
            entitlements: {
              pro: { active: true, ends_at: now + 29 * 86_400_000 },
              model: { active: true, plan: "模型 · 基础", quota: 2.45, used: 0.38, remaining: 2.07, cycle_end: now + 29 * 86_400_000, ends_at: now + 29 * 86_400_000 },
              currency: "CNY",
            },
          };
        case "plugins_list":
          return plugins;
        case "panel_state":
          return sp.get("state") === "thinking"
            ? { kind: "ask", title: "Rust 所有权与借用", pinned: false, turns: [], content: null, pending: "", thinking: true, error: null }
            : { kind: "ask", title: "Rust 所有权与借用 - Rust 程序设计语言", pinned: false, turns: [{ role: "assistant", content: answer }, { role: "user", content: "那 Copy 类型呢？" }, { role: "assistant", content: "实现了 `Copy` 的类型（整数、bool、char 等）赋值时按位复制，原变量仍可用。" }], content: null, pending: null, thinking: false, error: null };
        case "mark_restore":
          return `已打开 ${marks.find((m) => m.id === a.id)?.url ?? ""}`;
        default:
          return null;
      }
    },
    { shouldMockEvents: true },
  );
}
