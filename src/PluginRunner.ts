/**
 * 插件运行器：在隐藏的 plugin-N 窗口里执行插件代码。
 * 这个窗口只被授予 plugin_* 命令；每个能力再按插件声明的权限在 Rust 端检查。
 *
 * 插件 main.js 是一个 ES 模块，导出 run(ctx, api)（或 default.run）。
 */
import { invoke } from "@tauri-apps/api/core";

type Msg = { role: "user" | "assistant"; content: unknown };

const api = {
  /** 截取当前屏幕（标出了唤出点），返回 JPEG data URL。需要 screenshot 权限 */
  screenshot: () => invoke<string>("plugin_screenshot"),
  /** 通用模型接口，返回正文。需要 llm 权限，消耗用户的模型额度 */
  llm: (opts: { system?: string; messages: Msg[]; max_tokens?: number }) => invoke<string>("plugin_llm", { body: opts }),
  /** 在唤出点弹提示。status：done / pending / failed */
  toast: (title: string, detail = "", status: "done" | "pending" | "failed" = "done") =>
    invoke<void>("plugin_toast", { title, detail, status }),
  /** 在唤出点旁的悬浮面板里显示 Markdown，用户可继续追问 */
  show: (title: string, markdown: string) => invoke<void>("plugin_show", { title, markdown }),
  /** 写剪贴板。需要 clipboard 权限 */
  copy: (text: string) => invoke<void>("plugin_clipboard", { text }),
  /** 打开 http(s) 链接。需要 open 权限 */
  open: (url: string) => invoke<void>("plugin_open", { url }),
  /** 插件自己的键值存储 */
  storage: {
    get: <T = unknown>(key: string) => invoke<T | null>("plugin_storage_get", { key }),
    set: (key: string, value: unknown) => invoke<void>("plugin_storage_set", { key, value }),
  },
};

export async function runPlugin() {
  try {
    const [source, ctx] = await Promise.all([invoke<string>("plugin_source"), invoke<unknown>("plugin_ctx")]);
    const url = URL.createObjectURL(new Blob([source], { type: "text/javascript" }));
    const mod = await import(/* @vite-ignore */ url);
    URL.revokeObjectURL(url);
    const run = mod.run ?? mod.default?.run ?? mod.default;
    if (typeof run !== "function") throw new Error("插件没有导出 run(ctx, api)");
    await run(ctx, api);
    await invoke("plugin_finish", { error: null });
  } catch (e) {
    await invoke("plugin_finish", { error: e instanceof Error ? e.message : String(e) });
  }
}
