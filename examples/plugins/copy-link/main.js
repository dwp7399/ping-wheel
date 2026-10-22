// 复制为 Markdown 链接：只用场景信息和剪贴板，不调用模型。
export async function run(ctx, api) {
  const s = ctx.scene ?? {};
  if (!s.url) throw new Error("当前窗口不是网页，读不到链接");
  const title = (s.title || s.url).replace(/[[\]]/g, "");
  await api.copy(`[${title}](${s.url})`);
  await api.toast("已复制", title);
}
