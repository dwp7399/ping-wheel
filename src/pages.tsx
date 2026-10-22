import { useEffect, useMemo, useState } from "react";
import { Collection, Mark, SettingsView, Usage, api, on, timeText } from "./api";
import Shot from "./Shot";

function useFlash() {
  const [msg, setMsg] = useState<{ ok: boolean; text: string } | null>(null);
  useEffect(() => {
    if (!msg) return;
    const t = setTimeout(() => setMsg(null), 3000);
    return () => clearTimeout(t);
  }, [msg]);
  return [msg, setMsg] as const;
}

function Flash({ msg }: { msg: { ok: boolean; text: string } | null }) {
  return msg ? <span className={msg.ok ? "ok" : "err"}>{msg.text}</span> : null;
}

// ---------- 标记 ----------

function markWhere(m: Mark) {
  if (m.url) return { kind: "网页", text: m.url };
  if (m.doc_path) return { kind: "文件", text: m.doc_path };
  return { kind: "应用", text: m.app_name };
}

export function MarksPage() {
  const [marks, setMarks] = useState<Mark[]>([]);
  const [msg, setMsg] = useFlash();
  const reload = () => api.marks().then(setMarks);

  useEffect(() => {
    reload();
    return on("marks:changed", reload);
  }, []);

  return (
    <section>
      <div className="page-head">
        <h2>
          标记 <span className="muted">{marks.length} 条，点「回到这里」打开当时的网页、文件或应用</span>
        </h2>
        <Flash msg={msg} />
      </div>
      {marks.length === 0 && <p className="muted empty">还没有标记。用轮盘的「标记」记下当前在看的网页或文件。</p>}
      <div className="cards">
        {marks.map((m) => {
          const where = markWhere(m);
          return (
            <div key={m.id} className="card">
              {m.image_path ? <Shot className="card-shot" path={m.image_path} /> : <div className="card-shot shot-placeholder" />}
              <div className="card-body">
                <div className="card-title" title={m.window_title ?? ""}>
                  {m.window_title || m.app_name}
                </div>
                <div className="card-meta">
                  <span className="pill">{where.kind}</span>
                  <span className="ellipsis" title={where.text}>
                    {where.text}
                  </span>
                </div>
                <input
                  className="card-note"
                  defaultValue={m.note}
                  placeholder="备注…"
                  onBlur={(e) => e.target.value !== m.note && api.markNote(m.id, e.target.value)}
                />
                <div className="card-actions">
                  <span className="muted">
                    {m.app_name} · {timeText(m.created_at)}
                  </span>
                  <button
                    className="primary"
                    onClick={() =>
                      api
                        .markRestore(m.id)
                        .then((t) => setMsg({ ok: true, text: t }))
                        .catch((e) => setMsg({ ok: false, text: String(e) }))
                    }
                  >
                    回到这里
                  </button>
                  <button title="删除" onClick={() => api.markDelete(m.id).then(reload)}>
                    ✕
                  </button>
                </div>
              </div>
            </div>
          );
        })}
      </div>
    </section>
  );
}

// ---------- 收藏 ----------

const STATUS: Record<Collection["status"], string> = {
  pending: "等待归类",
  classifying: "归类中…",
  done: "",
  failed: "归类失败",
};

function sourceLabel(c: Collection) {
  if (c.url) {
    try {
      const u = new URL(c.url);
      return u.hostname + u.pathname;
    } catch {
      return c.url;
    }
  }
  if (c.doc_path) return c.doc_path.split("/").pop() ?? c.doc_path;
  return `${c.app_name}${c.window_title ? ` · ${c.window_title}` : ""}`;
}

export function CollectionsPage() {
  const [items, setItems] = useState<Collection[]>([]);
  const [filter, setFilter] = useState<string | null>(null);
  const [msg, setMsg] = useFlash();
  const reload = () => api.collections().then(setItems);

  useEffect(() => {
    reload();
    return on("collections:changed", reload);
  }, []);

  // 分类 → 来源 → 条目
  const groups = useMemo(() => {
    const byCat = new Map<string, Map<string, Collection[]>>();
    for (const c of items) {
      const cat = c.category || "未归类";
      if (filter && cat !== filter) continue;
      const bySrc = byCat.get(cat) ?? new Map<string, Collection[]>();
      bySrc.set(c.source_key, [...(bySrc.get(c.source_key) ?? []), c]);
      byCat.set(cat, bySrc);
    }
    return [...byCat.entries()];
  }, [items, filter]);

  const cats = useMemo(() => {
    const m = new Map<string, number>();
    items.forEach((c) => m.set(c.category || "未归类", (m.get(c.category || "未归类") ?? 0) + 1));
    return [...m.entries()].sort((a, b) => b[1] - a[1]);
  }, [items]);

  return (
    <section>
      <div className="page-head">
        <h2>
          收藏 <span className="muted">{items.length} 条，AI 自动归类，同一来源放在一起</span>
        </h2>
        <Flash msg={msg} />
      </div>
      {cats.length > 0 && (
        <div className="chips-row">
          <button className={filter === null ? "on" : ""} onClick={() => setFilter(null)}>
            全部
          </button>
          {cats.map(([c, n]) => (
            <button key={c} className={filter === c ? "on" : ""} onClick={() => setFilter(c)}>
              {c} <span className="muted">{n}</span>
            </button>
          ))}
        </div>
      )}
      {items.length === 0 && <p className="muted empty">还没有收藏。用轮盘的「收藏」截图收藏当前内容。</p>}
      {groups.map(([cat, sources]) => (
        <div key={cat} className="coll-cat">
          <h3>{cat}</h3>
          {[...sources.entries()].map(([key, list]) => (
            <div key={key} className="coll-source">
              <div className="coll-source-head">
                <span className="ellipsis" title={list[0].url ?? list[0].doc_path ?? key}>
                  {sourceLabel(list[0])}
                </span>
                {list.length > 1 && <span className="pill">{list.length} 张</span>}
                <button
                  onClick={() =>
                    api.collectionOpenSource(list[0].id).catch((e) => setMsg({ ok: false, text: String(e) }))
                  }
                >
                  打开来源
                </button>
              </div>
              <div className="cards">
                {list.map((c) => (
                  <div key={c.id} className="card">
                    <Shot className="card-shot" path={c.image_path} />
                    <div className="card-body">
                      <div className="card-title">{c.title}</div>
                      {STATUS[c.status] && <div className={`card-status s-${c.status}`}>{STATUS[c.status]}</div>}
                      {c.summary && <div className="card-summary">{c.summary}</div>}
                      {c.tags.length > 0 && (
                        <div className="card-tags">
                          {c.tags.map((t) => (
                            <span key={t} className="pill">
                              {t}
                            </span>
                          ))}
                        </div>
                      )}
                      <div className="card-actions">
                        <span className="muted">{timeText(c.created_at)}</span>
                        <button title="重新归类" onClick={() => api.collectionReclassify(c.id)}>
                          重新归类
                        </button>
                        <button title="删除" onClick={() => api.collectionDelete(c.id).then(reload)}>
                          ✕
                        </button>
                      </div>
                    </div>
                  </div>
                ))}
              </div>
            </div>
          ))}
        </div>
      ))}
    </section>
  );
}

// ---------- 模型设置 ----------

export function ModelPage() {
  const [view, setView] = useState<SettingsView | null>(null);
  const [baseUrl, setBaseUrl] = useState("");
  const [model, setModel] = useState("");
  const [key, setKey] = useState("");
  const [usage, setUsage] = useState<Usage | null>(null);
  const [msg, setMsg] = useFlash();
  const [testing, setTesting] = useState(false);

  useEffect(() => {
    api.settings().then((v) => {
      setView(v);
      setBaseUrl(v.base_url);
      setModel(v.model);
    });
    api.usageToday().then(setUsage);
  }, []);

  const save = async () => {
    try {
      const v = await api.setLlm(baseUrl, model, key.trim() ? key.trim() : null);
      setView(v);
      setKey("");
      setMsg({ ok: true, text: "已保存" });
    } catch (e) {
      setMsg({ ok: false, text: String(e) });
    }
  };

  const test = async () => {
    setTesting(true);
    try {
      setMsg({ ok: true, text: await api.llmTest() });
    } catch (e) {
      setMsg({ ok: false, text: String(e) });
    } finally {
      setTesting(false);
      api.usageToday().then(setUsage);
    }
  };

  if (!view) return null;
  return (
    <section>
      <div className="page-head">
        <h2>
          模型设置 <span className="muted">OpenAI 兼容接口，所有功能共用</span>
        </h2>
        <Flash msg={msg} />
      </div>
      <div className="form">
        <label>
          <span>接口地址</span>
          <input value={baseUrl} onChange={(e) => setBaseUrl(e.target.value)} placeholder="https://…/v3" />
        </label>
        <label>
          <span>模型</span>
          <input value={model} onChange={(e) => setModel(e.target.value)} placeholder="需支持图片输入" />
        </label>
        <label>
          <span>API Key</span>
          <input
            type="password"
            value={key}
            onChange={(e) => setKey(e.target.value)}
            placeholder={view.has_key ? `已填写（${view.key_hint}），留空不修改` : "还没有填写"}
          />
        </label>
        <div className="form-actions">
          <button className="primary" onClick={save}>
            保存
          </button>
          <button onClick={test} disabled={testing}>
            {testing ? "测试中…" : "测试连接"}
          </button>
        </div>
        <p className="muted">
          当前默认对接火山方舟 Agent Plan（地址 …/api/plan/v3，需用 Agent Plan 专属 Key）。Key 只保存在本机配置文件里。
        </p>
        {usage && (
          <p className="muted">
            今日调用 {usage.calls} 次（失败 {usage.failed}），输入 {usage.prompt_tokens.toLocaleString()} / 输出{" "}
            {usage.completion_tokens.toLocaleString()} tokens
          </p>
        )}
      </div>
    </section>
  );
}
