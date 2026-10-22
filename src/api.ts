import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export type Conversation = { id: number; title: string; updated_at: number };
export type ChatMessage = {
  id: number;
  role: "user" | "assistant";
  content: string;
  reasoning: string;
  image_path: string | null;
  status: "done" | "streaming" | "error";
  created_at: number;
};
export type Mark = {
  id: number;
  created_at: number;
  app_name: string;
  app_path: string | null;
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
  status: "pending" | "classifying" | "done" | "failed";
};
export type SettingsView = { base_url: string; model: string; has_key: boolean; key_hint: string };
export type Usage = { calls: number; failed: number; prompt_tokens: number; completion_tokens: number };

export const api = {
  chatNew: () => invoke<number>("chat_new"),
  chatList: () => invoke<Conversation[]>("chat_list"),
  chatMessages: (conversationId: number) => invoke<ChatMessage[]>("chat_messages", { conversationId }),
  chatSend: (conversationId: number, text: string) => invoke<void>("chat_send", { conversationId, text }),
  chatDelete: (conversationId: number) => invoke<void>("chat_delete", { conversationId }),
  chatOpenWindow: (conversationId?: number) => invoke<void>("chat_open_window", { conversationId }),

  marks: () => invoke<Mark[]>("marks_list"),
  markRestore: (id: number) => invoke<string>("mark_restore", { id }),
  markDelete: (id: number) => invoke<void>("mark_delete", { id }),
  markNote: (id: number, note: string) => invoke<void>("mark_note", { id, note }),

  collections: () => invoke<Collection[]>("collections_list"),
  collectionDelete: (id: number) => invoke<void>("collection_delete", { id }),
  collectionReclassify: (id: number) => invoke<void>("collection_reclassify", { id }),
  collectionOpenSource: (id: number) => invoke<void>("collection_open_source", { id }),

  settings: () => invoke<SettingsView>("settings_get"),
  setLlm: (baseUrl: string, model: string, apiKey: string | null) =>
    invoke<SettingsView>("settings_set_llm", { baseUrl, model, apiKey }),
  llmTest: () => invoke<string>("llm_test"),
  usageToday: () => invoke<Usage>("usage_today"),
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
    ? d.toLocaleTimeString("zh-CN", { hour: "2-digit", minute: "2-digit" })
    : d.toLocaleString("zh-CN", { month: "numeric", day: "numeric", hour: "2-digit", minute: "2-digit" });
}
