import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { IconName } from "./types";
import { getLang } from "./i18n";

export type Mark = {
  id: number;
  created_at: number;
  app_name: string;
  app_path: string | null;
  bundle_id: string | null;
  window_title: string | null;
  url: string | null;
  doc_path: string | null;
  image_path: string | null;
  note: string;
};
export type Collection = {
  id: number;
  created_at: number;
  image_path: string;
  app_name: string;
  app_path: string | null;
  window_title: string | null;
  url: string | null;
  doc_path: string | null;
  source_key: string;
  title: string;
  category: string;
  tags: string[];
  summary: string;
  status: "pending" | "classifying" | "done" | "failed" | "local";
};
export type AccountView = { server_url: string; has_token: boolean; token_hint: string };
export type LocaleView = { setting: string; effective: string; timezone: string; langs: [string, string][] };
export type Entitlements = {
  pro: { active: boolean; ends_at: number | null };
  model: {
    active: boolean;
    plan: string | null;
    quota: number;
    used: number;
    remaining: number;
    cycle_end: number | null;
    ends_at: number | null;
  };
  currency: string;
};
export type Me = { user: { id: number; name: string; email: string }; entitlements: Entitlements };

export type PluginAction = { id: string; label: string; icon: IconName };
export type PluginManifest = {
  id: string;
  name: string;
  version: string;
  description: string;
  author: string;
  permissions: string[];
  actions: PluginAction[];
};
export type PluginInfo = { manifest: PluginManifest; permission_labels: string[] };

export type Turn = { role: "user" | "assistant"; content: string };
export type PanelState = {
  kind: "ask" | "chat" | "result";
  title: string;
  pinned: boolean;
  turns: Turn[];
  content: string | null;
  pending: string | null;
  thinking: boolean;
  error: string | null;
};
export type PanelEvent = { type: "thinking" | "text" | "done" | "error"; text: string };

export const api = {
  marks: () => invoke<Mark[]>("marks_list"),
  markRestore: (id: number) => invoke<string>("mark_restore", { id }),
  markDelete: (id: number) => invoke<void>("mark_delete", { id }),
  markNote: (id: number, note: string) => invoke<void>("mark_note", { id, note }),

  collections: () => invoke<Collection[]>("collections_list"),
  collectionDelete: (id: number) => invoke<void>("collection_delete", { id }),
  collectionReclassify: (id: number) => invoke<void>("collection_reclassify", { id }),
  collectionOpenSource: (id: number) => invoke<void>("collection_open_source", { id }),

  account: () => invoke<AccountView>("account_get"),
  setAccount: (serverUrl: string, token: string | null) => invoke<AccountView>("account_set", { serverUrl, token }),
  me: () => invoke<Me>("account_me"),
  logout: () => invoke<void>("account_logout"),
  loginStart: (email: string) => invoke<void>("login_email_start", { email }),
  loginVerify: (email: string, code: string) => invoke<Me>("login_email_verify", { email, code }),
  locale: () => invoke<LocaleView>("locale_get"),
  setLocale: (language: string) => invoke<LocaleView>("locale_set", { language }),

  chatOpen: () => invoke<void>("chat_open"),

  plugins: () => invoke<PluginInfo[]>("plugins_list"),
  pluginInstall: (path: string) => invoke<PluginManifest>("plugin_install", { path }),
  pluginUninstall: (id: string) => invoke<void>("plugin_uninstall", { id }),

  panelState: () => invoke<PanelState>("panel_state"),
  panelSend: (text: string) => invoke<void>("panel_send", { text }),
  panelPin: (pinned: boolean) => invoke<void>("panel_pin", { pinned }),
  panelClose: () => invoke<void>("panel_close"),
};

/** 截图按路径缓存成 data URL */
const imageCache = new Map<string, Promise<string>>();
export function imageUrl(path: string): Promise<string> {
  let p = imageCache.get(path);
  if (!p) {
    p = invoke<string>("image_data_url", { path });
    imageCache.set(path, p);
    p.catch(() => imageCache.delete(path));
  }
  return p;
}

export function on<T>(event: string, cb: (payload: T) => void) {
  const off = listen<T>(event, (e) => cb(e.payload));
  return () => {
    off.then((f) => f());
  };
}

export function timeText(ms: number) {
  const d = new Date(ms);
  const now = new Date();
  const sameDay = d.toDateString() === now.toDateString();
  return sameDay
    ? d.toLocaleTimeString(getLang(), { hour: "2-digit", minute: "2-digit" })
    : d.toLocaleString(getLang(), { month: "numeric", day: "numeric", hour: "2-digit", minute: "2-digit" });
}

export function dateText(ms: number | null) {
  return ms ? new Date(ms).toLocaleDateString(getLang()) : "-";
}

export function money(n: number, currency: string) {
  const v = Number(n ?? 0).toFixed(2);
  return currency === "CNY" ? `¥${v}` : `${v} ${currency}`;
}
