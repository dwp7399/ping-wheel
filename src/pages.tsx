import { useEffect, useMemo, useState } from "react";
import { AccountView, Collection, Mark, Me, PluginInfo, api, dateText, money, on, timeText } from "./api";
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

// ---------- 账号 ----------

export function AccountPage() {
  const [view, setView] = useState<AccountView | null>(null);
  const [server, setServer] = useState("");
  const [token, setToken] = useState("");
  const [me, setMe] = useState<Me | null>(null);
  const [meErr, setMeErr] = useState("");
  const [msg, setMsg] = useFlash();

  const refresh = () =>
    api
      .me()
      .then((m) => {
        setMe(m);
        setMeErr("");
      })
      .catch((e) => {
        setMe(null);
        setMeErr(String(e));
      });

  useEffect(() => {
    api.account().then((v) => {
      setView(v);
      setServer(v.server_url);
      if (v.has_token) refresh();
    });
  }, []);

  const save = async () => {
    try {
      const v = await api.setAccount(server, token.trim() ? token.trim() : null);
      setView(v);
      setToken("");
      setMsg({ ok: true, text: "已保存" });
      refresh();
    } catch (e) {
      setMsg({ ok: false, text: String(e) });
    }
  };

  if (!view) return null;
  const e = me?.entitlements;
  return (
    <section>
      <div className="page-head">
        <h2>
          账号 <span className="muted">模型能力和高级功能由 Ping Wheel 服务提供</span>
        </h2>
        <Flash msg={msg} />
      </div>

      {me && e && (
        <div className="plan-cards">
          <div className={`plan-card ${e.model.active ? "on" : ""}`}>
            <div className="plan-name">模型能力</div>
            {e.model.active ? (
              <>
                <div className="plan-status">{e.model.plan}，{dateText(e.model.ends_at)} 到期</div>
                <div className="quota">
                  <i style={{ width: `${Math.min(100, (e.model.used / Math.max(e.model.quota, 0.0001)) * 100)}%` }} />
                </div>
                <div className="muted">
                  本周期已用 {money(e.model.used, e.currency)} / {money(e.model.quota, e.currency)}，
                  {dateText(e.model.cycle_end)} 重置
                </div>
              </>
            ) : (
              <div className="muted">未订阅。问号、对话、收藏归类和插件的模型调用都需要模型订阅（¥4.9 / ¥9.9 / ¥19.9 每月）。</div>
            )}
          </div>
          <div className={`plan-card ${e.pro.active ? "on" : ""}`}>
            <div className="plan-name">高级功能</div>
            {e.pro.active ? (
              <div className="plan-status">已解锁自定义插件，{dateText(e.pro.ends_at)} 到期</div>
            ) : (
              <div className="muted">未订阅。解锁自定义插件（¥0.99 每月）。</div>
            )}
          </div>
        </div>
      )}
      {meErr && <p className="err">{meErr}</p>}

      <div className="form">
        {me && (
          <p className="muted" style={{ paddingLeft: 0 }}>
            已登录：{me.user.name} {me.user.email && `（${me.user.email}）`}
          </p>
        )}
        <label>
          <span>服务地址</span>
          <input value={server} onChange={(e) => setServer(e.target.value)} placeholder="https://…" />
        </label>
        <label>
          <span>账号令牌</span>
          <input
            type="password"
            value={token}
            onChange={(e) => setToken(e.target.value)}
            placeholder={view.has_token ? `已填写（${view.token_hint}），留空不修改` : "pw_…"}
          />
        </label>
        <div className="form-actions">
          <button className="primary" onClick={save}>
            保存
          </button>
          <button onClick={refresh} disabled={!view.has_token}>
            刷新订阅
          </button>
        </div>
      </div>
    </section>
  );
}

// ---------- 插件 ----------

export function PluginsPage() {
  const [plugins, setPlugins] = useState<PluginInfo[]>([]);
  const [pro, setPro] = useState<boolean | null>(null);
  const [msg, setMsg] = useFlash();
  const reload = () => api.plugins().then(setPlugins);

  useEffect(() => {
    reload();
    api
      .me()
      .then((m) => setPro(m.entitlements.pro.active))
      .catch(() => setPro(false));
    return on("plugins:changed", reload);
  }, []);

  const install = async () => {
    const { open } = await import("@tauri-apps/plugin-dialog");
    const path = await open({ multiple: false, filters: [{ name: "插件包", extensions: ["zip"] }] });
    if (!path) return;
    try {
      const m = await api.pluginInstall(path as string);
      setMsg({ ok: true, text: `已安装「${m.name}」${m.version}，可在「轮盘」里把它的动作放到某个方向上` });
    } catch (e) {
      setMsg({ ok: false, text: String(e) });
    }
  };

  return (
    <section>
      <div className="page-head">
        <h2>
          插件 <span className="muted">以 zip 包安装，装好后在「轮盘」里放到某个方向上</span>
        </h2>
        <div className="row-gap">
          <Flash msg={msg} />
          <button className="primary" onClick={install} disabled={pro === false}>
            从 zip 安装…
          </button>
        </div>
      </div>
      {pro === false && <p className="notice">自定义插件需要订阅「高级功能」（¥0.99 每月），请到「账号」页查看。</p>}
      {plugins.length === 0 && <p className="muted empty">还没有安装插件。</p>}
      <div className="plugin-list">
        {plugins.map((p) => (
          <div key={p.manifest.id} className="plugin-card">
            <div className="plugin-head">
              <b>{p.manifest.name}</b>
              <span className="muted">
                {p.manifest.version} · {p.manifest.id}
                {p.manifest.author && ` · ${p.manifest.author}`}
              </span>
              <button
                className="danger"
                onClick={() => {
                  if (confirm(`卸载「${p.manifest.name}」？它在轮盘上的动作也会一并移除。`))
                    api.pluginUninstall(p.manifest.id).catch((e) => setMsg({ ok: false, text: String(e) }));
                }}
              >
                卸载
              </button>
            </div>
            {p.manifest.description && <div className="plugin-desc">{p.manifest.description}</div>}
            <div className="plugin-meta">
              <span className="muted">动作：</span>
              {p.manifest.actions.map((a) => (
                <span key={a.id} className="pill">
                  {a.label}
                </span>
              ))}
            </div>
            {p.permission_labels.length > 0 && (
              <div className="plugin-meta">
                <span className="muted">权限：</span>
                {p.permission_labels.join("；")}
              </div>
            )}
          </div>
        ))}
      </div>
    </section>
  );
}
