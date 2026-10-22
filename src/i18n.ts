/**
 * 界面文案翻译：以简体中文原文为键，查 i18n/<语言>.json（与 Rust 共用）。
 * 简体 / 繁体中文显示原文；其他语言暂时都用英文表；查不到就显示原文。
 * 语言由 Rust 端决定（按系统时区或用户设置），切换语言后各窗口重新加载。
 */
import en from "../i18n/en.json";

const TABLES: Record<string, Record<string, string>> = { en };

let lang = "zh-CN";

export function getLang() {
  return lang;
}

function table(): Record<string, string> | null {
  if (lang.startsWith("zh")) return null;
  return TABLES[lang] ?? TABLES.en;
}

export function t(zh: string, vars?: Record<string, string | number>): string {
  let s = table()?.[zh] ?? zh;
  if (vars) for (const [k, v] of Object.entries(vars)) s = s.split(`{${k}}`).join(String(v));
  return s;
}

/** 启动时调用：Tauri 下向 Rust 取语言；浏览器预览下可用 ?lang=en 指定 */
export async function initLocale(inTauri: boolean) {
  if (!inTauri) {
    lang = new URLSearchParams(location.search).get("lang") ?? "zh-CN";
    document.documentElement.lang = lang;
    return;
  }
  const { invoke } = await import("@tauri-apps/api/core");
  const { listen } = await import("@tauri-apps/api/event");
  try {
    const v = await invoke<{ effective: string }>("locale_get");
    lang = v.effective;
  } catch {
    lang = "en";
  }
  document.documentElement.lang = lang;
  listen("locale:changed", () => location.reload());
}
