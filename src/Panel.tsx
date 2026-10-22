import { useEffect, useRef, useState } from "react";
import Markdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { PanelEvent, PanelState, Turn, api, on } from "./api";
import { Icon } from "./icons";

/**
 * 悬浮面板：问号的理解结果 / 对话 / 插件结果。
 * 失去焦点即关闭（钉住除外），关闭即结束；只显示「思考中」和回答正文。
 */
export default function Panel() {
  const [state, setState] = useState<PanelState | null>(null);
  const [input, setInput] = useState("");
  const [gone, setGone] = useState(false);
  const pinnedRef = useRef(false);
  const listRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLTextAreaElement>(null);
  const stick = useRef(true);

  useEffect(() => {
    // 先订阅事件再取状态，避免错过面板刚打开时的输出
    const off = on<PanelEvent>("panel:event", (e) => {
      setState((s) => {
        if (!s) return s;
        switch (e.type) {
          case "thinking":
            return { ...s, thinking: true, pending: s.pending ?? "" };
          case "text":
            return { ...s, pending: e.text };
          case "done":
            return { ...s, pending: null, thinking: false, turns: [...s.turns, { role: "assistant", content: e.text }] };
          case "error":
            return {
              ...s,
              pending: null,
              thinking: false,
              error: e.text,
              turns: s.turns.length && s.turns[s.turns.length - 1].role === "user" ? s.turns.slice(0, -1) : s.turns,
            };
        }
      });
    });
    api
      .panelState()
      .then((s) => {
        pinnedRef.current = s.pinned;
        setState((cur) => cur ?? s);
      })
      .catch(() => setGone(true));

    // 失去焦点就关闭（钉住时保留）
    const win = getCurrentWindow();
    const unFocus = win.onFocusChanged(({ payload: focused }) => {
      if (!focused && !pinnedRef.current) api.panelClose();
    });
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") api.panelClose();
    };
    window.addEventListener("keydown", onKey);
    return () => {
      off();
      unFocus.then((f) => f());
      window.removeEventListener("keydown", onKey);
    };
  }, []);

  useEffect(() => {
    const el = listRef.current;
    if (el && stick.current) el.scrollTop = el.scrollHeight;
  }, [state]);

  useEffect(() => {
    if (state && state.kind !== "ask") inputRef.current?.focus();
  }, [state?.kind]);

  if (gone) return null;
  if (!state) return <div className="panel" />;

  const busy = state.pending !== null;
  const pin = () => {
    const next = !state.pinned;
    pinnedRef.current = next;
    setState({ ...state, pinned: next });
    api.panelPin(next);
  };
  const send = async () => {
    const text = input.trim();
    if (!text || busy) return;
    setInput("");
    stick.current = true;
    setState({ ...state, error: null, pending: "", turns: [...state.turns, { role: "user", content: text }] });
    try {
      await api.panelSend(text);
    } catch (e) {
      setState((s) => (s ? { ...s, pending: null, error: String(e) } : s));
    }
  };

  // 插件结果作为第一条回答显示
  const turns: Turn[] = state.content ? [{ role: "assistant", content: state.content }, ...state.turns] : state.turns;
  const empty = turns.length === 0 && !busy && !state.error;

  return (
    <div className="panel">
      <header className="panel-head" data-tauri-drag-region>
        <span className="panel-badge" data-tauri-drag-region>
          {state.kind === "ask" ? "?" : <Icon name={state.kind === "chat" ? "chat" : "sparkle"} size={14} />}
        </span>
        <span className="panel-title" data-tauri-drag-region title={state.title}>
          {state.title || (state.kind === "chat" ? "对话" : "")}
        </span>
        <button className={`panel-icon ${state.pinned ? "on" : ""}`} onClick={pin} title={state.pinned ? "取消钉住" : "钉在桌面上"}>
          <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.9" strokeLinecap="round" strokeLinejoin="round">
            <path d="M9 4h6l-1 6 3 3H7l3-3-1-6Z" fill={state.pinned ? "currentColor" : "none"} />
            <path d="M12 13v7" />
          </svg>
        </button>
        <button className="panel-icon" onClick={() => api.panelClose()} title="关闭（Esc）">
          ✕
        </button>
      </header>

      <div
        ref={listRef}
        className="panel-body"
        onScroll={(e) => {
          const el = e.currentTarget;
          stick.current = el.scrollHeight - el.scrollTop - el.clientHeight < 40;
        }}
      >
        {empty && <div className="panel-empty muted">有什么想问的？Enter 发送，Shift+Enter 换行</div>}
        {turns.map((t, i) =>
          t.role === "user" ? (
            <div key={i} className="p-user">
              {t.content}
            </div>
          ) : (
            <div key={i} className="p-answer md">
              <Markdown remarkPlugins={[remarkGfm]}>{t.content}</Markdown>
            </div>
          ),
        )}
        {busy &&
          (state.pending ? (
            <div className="p-answer md">
              <Markdown remarkPlugins={[remarkGfm]}>{state.pending}</Markdown>
            </div>
          ) : (
            <div className="p-thinking">
              <span className="dots">
                <i />
                <i />
                <i />
              </span>
              思考中
            </div>
          ))}
        {state.error && <div className="p-error">{state.error}</div>}
      </div>

      <div className="panel-input">
        <textarea
          ref={inputRef}
          value={input}
          rows={1}
          placeholder={busy ? "正在回答…" : state.kind === "ask" ? "继续追问…" : "输入问题…"}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing) {
              e.preventDefault();
              send();
            }
          }}
        />
        <button className="panel-send" disabled={!input.trim() || busy} onClick={send} title="发送">
          ↑
        </button>
      </div>
    </div>
  );
}
