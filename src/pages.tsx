import { useEffect, useMemo, useState } from "react";
import { AccountView, Collection, LocaleView, Mark, Me, PluginInfo, api, dateText, money, on, timeText } from "./api";
import { t } from "./i18n";
import Shot from "./Shot";

function useFlash() {
  const [msg, setMsg] = useState<{ ok: boolean; text: string } | null>(null);
  useEffect(() => {
    if (!msg) return;
    const timer = setTimeout(() => setMsg(null), 3000);
    return () => clearTimeout(timer);
  }, [msg]);
  return [msg, setMsg] as const;
}

function Flash({ msg }: { msg: { ok: boolean; text: string } | null }) {
  return msg ? <span className={msg.ok ? "ok" : "err"}>{msg.text}</span> : null;
}

// ---------- 标记 ----------

function markWhere(m: Mark) {
  if (m.url) return { kind: t("网页"), text: m.url };
  if (m.doc_path) return { kind: t("文件"), text: m.doc_path };
  return { kind: t("应用"), text: m.app_name };
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
          {t("标记")}{" "}
          <span className="muted">{t("{n} 条，点「回到这里」打开当时的网页、文件或应用", { n: marks.length })}</span>
        </h2>
        <Flash msg={msg} />
      </div>
      {marks.length === 0 && <p className="muted empty">{t("还没有标记。用轮盘的「标记」记下当前在看的网页或文件。")}</p>}
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
                  placeholder={t("备注…")}
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
                        .then((text) => setMsg({ ok: true, text }))
                        .catch((e) => setMsg({ ok: false, text: String(e) }))
                    }
                  >
                    {t("回到这里")}
                  </button>
                  <button title={t("删除")} onClick={() => api.markDelete(m.id).then(reload)}>
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
  pending: t("等待归类"),
  classifying: t("归类中…"),
  done: "",
  failed: t("归类失败"),
  local: t("登录后可自动归类"),
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
  const uncategorized = t("未归类");

  useEffect(() => {
    reload();
    return on("collections:changed", reload);
  }, []);

  // 分类 → 来源 → 条目
  const groups = useMemo(() => {
    const byCat = new Map<string, Map<string, Collection[]>>();
    for (const c of items) {
      const cat = c.category || uncategorized;
      if (filter && cat !== filter) continue;
      const bySrc = byCat.get(cat) ?? new Map<string, Collection[]>();
      bySrc.set(c.source_key, [...(bySrc.get(c.source_key) ?? []), c]);
      byCat.set(cat, bySrc);
    }
    return [...byCat.entries()];
  }, [items, filter]);

  const cats = useMemo(() => {
    const m = new Map<string, number>();
    items.forEach((c) => m.set(c.category || uncategorized, (m.get(c.category || uncategorized) ?? 0) + 1));
    return [...m.entries()].sort((a, b) => b[1] - a[1]);
  }, [items]);

  return (
    <section>
      <div className="page-head">
        <h2>
          {t("收藏")} <span className="muted">{t("{n} 条，AI 自动归类，同一来源放在一起", { n: items.length })}</span>
        </h2>
        <Flash msg={msg} />
      </div>
      {cats.length > 0 && (
        <div className="chips-row">
          <button className={filter === null ? "on" : ""} onClick={() => setFilter(null)}>
            {t("全部")}
          </button>
          {cats.map(([c, n]) => (
            <button key={c} className={filter === c ? "on" : ""} onClick={() => setFilter(c)}>
              {c} <span className="muted">{n}</span>
            </button>
          ))}
        </div>
      )}
      {items.length === 0 && <p className="muted empty">{t("还没有收藏。用轮盘的「收藏」截图收藏当前内容。")}</p>}
      {groups.map(([cat, sources]) => (
        <div key={cat} className="coll-cat">
          <h3>{cat}</h3>
          {[...sources.entries()].map(([key, list]) => (
            <div key={key} className="coll-source">
              <div className="coll-source-head">
                <span className="ellipsis" title={list[0].url ?? list[0].doc_path ?? key}>
                  {sourceLabel(list[0])}
                </span>
                {list.length > 1 && <span className="pill">{t("{n} 张", { n: list.length })}</span>}
                <button
                  onClick={() =>
                    api.collectionOpenSource(list[0].id).catch((e) => setMsg({ ok: false, text: String(e) }))
                  }
                >
                  {t("打开来源")}
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
                          {c.tags.map((tag) => (
                            <span key={tag} className="pill">
                              {tag}
                            </span>
                          ))}
                        </div>
                      )}
                      <div className="card-actions">
                        <span className="muted">{timeText(c.created_at)}</span>
                        <button onClick={() => api.collectionReclassify(c.id)}>{t("重新归类")}</button>
                        <button title={t("删除")} onClick={() => api.collectionDelete(c.id).then(reload)}>
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

// ---------- 账号与语言 ----------

function LanguagePicker() {
  const [locale, setLocale] = useState<LocaleView | null>(null);
  useEffect(() => {
    api.locale().then(setLocale);
  }, []);
  if (!locale) return null;
  const autoName = locale.langs.find(([c]) => c === locale.effective)?.[1] ?? locale.effective;
  return (
    <label>
      <span>{t("语言")}</span>
      <select value={locale.setting} onChange={(e) => api.setLocale(e.target.value).then(setLocale)}>
        <option value="auto">
          {t("自动（按时区 {tz}）", { tz: locale.timezone || "?" })}
          {locale.setting === "auto" ? ` · ${autoName}` : ""}
        </option>
        {locale.langs.map(([code, name]) => (
          <option key={code} value={code}>
            {name}
          </option>
        ))}
      </select>
    </label>
  );
}

function EmailLogin({ onDone }: { onDone: (m: Me) => void }) {
  const [email, setEmail] = useState("");
  const [code, setCode] = useState("");
  const [sent, setSent] = useState(false);
  const [busy, setBusy] = useState(false);
  const [msg, setMsg] = useFlash();

  const start = async () => {
    setBusy(true);
    try {
      await api.loginStart(email);
      setSent(true);
      setMsg({ ok: true, text: t("验证码已发送到 {email}", { email }) });
    } catch (e) {
      setMsg({ ok: false, text: String(e) });
    } finally {
      setBusy(false);
    }
  };
  const verify = async () => {
    setBusy(true);
    try {
      onDone(await api.loginVerify(email, code));
    } catch (e) {
      setMsg({ ok: false, text: String(e) });
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="form">
      <label>
        <span>{t("邮箱")}</span>
        <input
          type="email"
          value={email}
          autoFocus
          onChange={(e) => setEmail(e.target.value)}
          placeholder="you@example.com"
          onKeyDown={(e) => e.key === "Enter" && !sent && start()}
        />
      </label>
      {sent && (
        <label>
          <span>{t("验证码")}</span>
          <input
            value={code}
            inputMode="numeric"
            maxLength={6}
            autoFocus
            onChange={(e) => setCode(e.target.value.replace(/\D/g, ""))}
            placeholder={t("6 位数字")}
            onKeyDown={(e) => e.key === "Enter" && code.length === 6 && verify()}
          />
        </label>
      )}
      <div className="form-actions">
        {sent ? (
          <>
            <button className="primary" disabled={busy || code.length !== 6} onClick={verify}>
              {t("登录")}
            </button>
            <button disabled={busy} onClick={start}>
              {t("重新发送")}
            </button>
          </>
        ) : (
          <button className="primary" disabled={busy || !email.includes("@")} onClick={start}>
            {t("发送验证码")}
          </button>
        )}
        <Flash msg={msg} />
      </div>
      <p className="muted">{t("没有账号会自动注册。")}</p>
    </div>
  );
}

export function AccountPage() {
  const [view, setView] = useState<AccountView | null>(null);
  const [me, setMe] = useState<Me | null>(null);
  const [meErr, setMeErr] = useState("");
  const [advanced, setAdvanced] = useState(false);
  const [server, setServer] = useState("");
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

  const saveServer = async () => {
    try {
      setView(await api.setAccount(server, null));
      setMsg({ ok: true, text: t("已保存") });
    } catch (e) {
      setMsg({ ok: false, text: String(e) });
    }
  };

  const logout = async () => {
    await api.logout();
    setMe(null);
    setView(await api.account());
  };

  if (!view) return null;
  const e = me?.entitlements;
  const loggedIn = view.has_token && !!me;
  return (
    <section>
      <div className="page-head">
        <h2>
          {t("账号")} <span className="muted">{t("模型能力和高级功能由 Ping Wheel 服务提供")}</span>
        </h2>
        <Flash msg={msg} />
      </div>

      {loggedIn && e && (
        <>
          <div className="account-line">
            <span>{t("已登录：{name}", { name: me.user.email || me.user.name })}</span>
            <button onClick={refresh}>{t("刷新订阅")}</button>
            <button onClick={logout}>{t("退出登录")}</button>
          </div>
          <div className="plan-cards">
            <div className={`plan-card ${e.model.active ? "on" : ""}`}>
              <div className="plan-name">{t("模型能力")}</div>
              {e.model.active ? (
                <>
                  <div className="plan-status">
                    {t("{plan}，{date} 到期", { plan: e.model.plan ?? "", date: dateText(e.model.ends_at) })}
                  </div>
                  <div className="quota">
                    <i style={{ width: `${Math.min(100, (e.model.used / Math.max(e.model.quota, 0.0001)) * 100)}%` }} />
                  </div>
                  <div className="muted">
                    {t("本周期已用 {used} / {quota}，{date} 重置", {
                      used: money(e.model.used, e.currency),
                      quota: money(e.model.quota, e.currency),
                      date: dateText(e.model.cycle_end),
                    })}
                  </div>
                </>
              ) : (
                <div className="muted">
                  {t("未订阅。?、对话、收藏归类和插件的模型调用都需要模型订阅（¥4.9 / ¥9.9 / ¥19.9，每 31 天）。")}
                </div>
              )}
            </div>
            <div className={`plan-card ${e.pro.active ? "on" : ""}`}>
              <div className="plan-name">{t("高级功能")}</div>
              {e.pro.active ? (
                <div className="plan-status">{t("已解锁自定义插件，{date} 到期", { date: dateText(e.pro.ends_at) })}</div>
              ) : (
                <div className="muted">{t("未订阅。¥0.99 / 31 天解锁自定义插件。说白了这是一杯奶茶的赞助费：客户端开源，想绕过这道检查并不难，但我们相信你会选择请我们喝一杯。")}</div>
              )}
            </div>
          </div>
        </>
      )}
      {view.has_token && meErr && <p className="err">{meErr}</p>}
      {!loggedIn && (
        <EmailLogin
          onDone={async (m) => {
            setMe(m);
            setMeErr("");
            setView(await api.account());
          }}
        />
      )}

      <h3 className="section-title">{t("偏好")}</h3>
      <div className="form">
        <LanguagePicker />
      </div>

      <h3 className="section-title">
        <button className="link" onClick={() => setAdvanced((v) => !v)}>
          {advanced ? "▾" : "▸"} {t("高级")}
        </button>
      </h3>
      {advanced && (
        <div className="form">
          <label>
            <span>{t("服务地址")}</span>
            <input value={server} onChange={(e) => setServer(e.target.value)} placeholder="https://…" />
          </label>
          <div className="form-actions">
            <button onClick={saveServer}>{t("保存")}</button>
          </div>
        </div>
      )}
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
    const path = await open({ multiple: false, filters: [{ name: t("插件包"), extensions: ["zip"] }] });
    if (!path) return;
    try {
      const m = await api.pluginInstall(path as string);
      setMsg({
        ok: true,
        text: t("已安装「{name}」{version}，可在「轮盘」里把它的动作放到某个方向上", { name: m.name, version: m.version }),
      });
    } catch (e) {
      setMsg({ ok: false, text: String(e) });
    }
  };

  return (
    <section>
      <div className="page-head">
        <h2>
          {t("插件")} <span className="muted">{t("以 zip 包安装，装好后在「轮盘」里放到某个方向上")}</span>
        </h2>
        <div className="row-gap">
          <Flash msg={msg} />
          <button className="primary" onClick={install} disabled={pro === false}>
            {t("从 zip 安装…")}
          </button>
        </div>
      </div>
      {pro === false && <p className="notice">{t("自定义插件需要「高级功能」（¥0.99 / 31 天）。悄悄说一句：这其实是赞助费，代码开源、拦不住你，但还是请我们喝杯奶茶吧。可以在「账号」页开通。")}</p>}
      {plugins.length === 0 && <p className="muted empty">{t("还没有安装插件。")}</p>}
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
                  if (confirm(t("卸载「{name}」？它在轮盘上的动作也会一并移除。", { name: p.manifest.name })))
                    api.pluginUninstall(p.manifest.id).catch((e) => setMsg({ ok: false, text: String(e) }));
                }}
              >
                {t("卸载")}
              </button>
            </div>
            {p.manifest.description && <div className="plugin-desc">{p.manifest.description}</div>}
            <div className="plugin-meta">
              <span className="muted">{t("动作：")}</span>
              {p.manifest.actions.map((a) => (
                <span key={a.id} className="pill">
                  {a.label}
                </span>
              ))}
            </div>
            {p.permission_labels.length > 0 && (
              <div className="plugin-meta">
                <span className="muted">{t("权限：")}</span>
                {p.permission_labels.join(t("；"))}
              </div>
            )}
          </div>
        ))}
      </div>
    </section>
  );
}
