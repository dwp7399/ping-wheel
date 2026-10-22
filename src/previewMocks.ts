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
const messages = [
  { id: 1, role: "user", content: "请解释截图里我正在看的内容：它是什么、关键点是什么、有什么需要注意的。", reasoning: "", image_path: "/shot/1.png", status: "done", created_at: now - 60_000 },
  { id: 2, role: "assistant", content: "这是 **Rust 所有权** 一章的开头。\n\n- 每个值有且只有一个所有者\n- 所有者离开作用域，值被丢弃\n- 赋值会发生 *move*，原变量失效\n\n```rust\nlet s1 = String::from(\"hi\");\nlet s2 = s1; // s1 不再可用\n```", reasoning: "用户在看 Rust 文档的所有权章节……", image_path: null, status: "done", created_at: now - 55_000 },
];

export function installPreviewMocks(label: string) {
  mockWindows(label, "main", "chat", "wheel", "toast");
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
        case "chat_list":
          return [{ id: 1, title: "问号 · Rust 所有权与借用", updated_at: now - 55_000 }];
        case "chat_messages":
          return messages;
        case "chat_new":
          return 99;
        case "image_data_url":
          return PIXEL;
        case "settings_get":
          return { base_url: "https://ark.cn-beijing.volces.com/api/plan/v3", model: "glm-5.3-flash", has_key: true, key_hint: "…a1b2" };
        case "usage_today":
          return { calls: 12, failed: 1, prompt_tokens: 18234, completion_tokens: 4410 };
        case "llm_test":
          return "连接正常（模型 glm-5-3-flash-260828）";
        case "mark_restore":
          return `已打开 ${marks.find((m) => m.id === a.id)?.url ?? ""}`;
        default:
          return null;
      }
    },
    { shouldMockEvents: true },
  );
}
