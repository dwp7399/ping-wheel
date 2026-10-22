import { useCallback, useEffect, useRef, useState } from "react";
import Markdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { ChatMessage, Conversation, api, on, timeText } from "./api";
import Shot from "./Shot";

type DeltaPayload = {
  conversation_id: number;
  message_id: number;
  content: string | null;
  reasoning: string | null;
};
type DonePayload = { conversation_id: number; message_id: number; status: ChatMessage["status"]; content: string };

export default function ChatWindow() {
  const [convId, setConvId] = useState<number | null>(null);
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [history, setHistory] = useState<Conversation[]>([]);
  const [showHistory, setShowHistory] = useState(false);
  const [input, setInput] = useState("");
  const [zoom, setZoom] = useState<string | null>(null);
  const convRef = useRef<number | null>(null);
  const listRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLTextAreaElement>(null);
  const stickToBottom = useRef(true);

  const load = useCallback(async (id: number) => {
    convRef.current = id;
    setConvId(id);
    setShowHistory(false);
    stickToBottom.current = true;
    setMessages(await api.chatMessages(id));
    setHistory(await api.chatList());
    inputRef.current?.focus();
  }, []);

  useEffect(() => {
    const offs = [
      on<number>("chat:open", (id) => load(id)),
      on<number>("chat:changed", (id) => {
        if (id === convRef.current) api.chatMessages(id).then(setMessages);
      }),
      on<DeltaPayload>("chat:delta", (d) => {
        if (d.conversation_id !== convRef.current) return;
        setMessages((list) =>
          list.map((m) =>
            m.id === d.message_id
              ? { ...m, content: m.content + (d.content ?? ""), reasoning: m.reasoning + (d.reasoning ?? "") }
              : m,
          ),
        );
      }),
      on<DonePayload>("chat:done", (d) => {
        if (d.conversation_id !== convRef.current) return;
        setMessages((list) =>
          list.map((m) => (m.id === d.message_id ? { ...m, status: d.status, content: d.content } : m)),
        );
      }),
    ];
    return () => offs.forEach((f) => f());
  }, [load]);

  // 新内容到来时，若用户停在底部就跟随滚动
  useEffect(() => {
    const el = listRef.current;
    if (el && stickToBottom.current) el.scrollTop = el.scrollHeight;
  }, [messages]);

  const busy = messages.some((m) => m.status === "streaming");

  const send = async () => {
    const text = input.trim();
    if (!text || busy) return;
    let id = convRef.current;
    if (id === null) {
      id = await api.chatNew();
      convRef.current = id;
      setConvId(id);
    }
    setInput("");
    stickToBottom.current = true;
    setMessages((list) => [
      ...list,
      { id: -Date.now(), role: "user", content: text, reasoning: "", image_path: null, status: "done", created_at: Date.now() },
    ]);
    try {
      await api.chatSend(id, text);
      setHistory(await api.chatList());
    } catch (e) {
      setMessages((list) => [
        ...list,
        { id: -Date.now() - 1, role: "assistant", content: String(e), reasoning: "", image_path: null, status: "error", created_at: Date.now() },
      ]);
    }
  };

  const newChat = async () => load(await api.chatNew());

  const openHistory = async () => {
    setHistory(await api.chatList());
    setShowHistory((v) => !v);
  };

  return (
    <div className="chat">
      <header className="chat-head" data-tauri-drag-region>
        <button className="chat-btn" onClick={openHistory} title="历史对话">
          历史
        </button>
        <span className="chat-title" data-tauri-drag-region>
          {history.find((h) => h.id === convId)?.title || "对话"}
        </span>
        <button className="chat-btn" onClick={newChat} title="新对话">
          新对话
        </button>
      </header>

      {showHistory && (
        <div className="chat-history">
          {history.length === 0 && <div className="muted">还没有对话</div>}
          {history.map((h) => (
            <div key={h.id} className={`chat-history-row ${h.id === convId ? "on" : ""}`}>
              <button className="chat-history-open" onClick={() => load(h.id)}>
                <span>{h.title || "未命名对话"}</span>
                <span className="muted">{timeText(h.updated_at)}</span>
              </button>
              <button
                className="chat-history-del"
                title="删除"
                onClick={async () => {
                  await api.chatDelete(h.id);
                  setHistory(await api.chatList());
                  if (h.id === convId) newChat();
                }}
              >
                ✕
              </button>
            </div>
          ))}
        </div>
      )}

      <div
        ref={listRef}
        className="chat-list"
        onScroll={(e) => {
          const el = e.currentTarget;
          stickToBottom.current = el.scrollHeight - el.scrollTop - el.clientHeight < 40;
        }}
      >
        {messages.length === 0 && (
          <div className="chat-empty">
            <b>有什么想问的？</b>
            <span className="muted">Enter 发送，Shift+Enter 换行。用轮盘的「问号」可以带上屏幕截图提问。</span>
          </div>
        )}
        {messages.map((m) => (
          <div key={m.id} className={`msg msg-${m.role} ${m.status === "error" ? "msg-error" : ""}`}>
            {m.image_path && <Shot className="msg-shot" path={m.image_path} onClick={() => setZoom(m.image_path)} />}
            {m.role === "assistant" && m.reasoning && (
              <details className="msg-think" open={m.status === "streaming" && !m.content}>
                <summary>{m.status === "streaming" && !m.content ? "思考中…" : "思考过程"}</summary>
                <div className="msg-think-body">{m.reasoning}</div>
              </details>
            )}
            {m.role === "assistant" ? (
              m.content ? (
                <div className="md">
                  <Markdown remarkPlugins={[remarkGfm]}>{m.content}</Markdown>
                </div>
              ) : m.status === "streaming" && !m.reasoning ? (
                <div className="msg-wait">正在思考…</div>
              ) : null
            ) : (
              <div className="msg-text">{m.content}</div>
            )}
          </div>
        ))}
      </div>

      <div className="chat-input">
        <textarea
          ref={inputRef}
          value={input}
          rows={2}
          placeholder={busy ? "正在回答…" : "继续追问…"}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing) {
              e.preventDefault();
              send();
            }
          }}
        />
        <button className="chat-send" disabled={!input.trim() || busy} onClick={send}>
          发送
        </button>
      </div>

      {zoom && (
        <div className="chat-zoom" onClick={() => setZoom(null)}>
          <Shot path={zoom} />
        </div>
      )}
    </div>
  );
}
