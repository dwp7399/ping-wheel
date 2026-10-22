// 标记翻译：优先翻译选中的文字，其次是标记处的文字；都读不到时让模型看截图里红圈处的内容。
export async function run(ctx, api) {
  const s = ctx.scene ?? {};
  const text = s.selection || s.focusText || s.nearbyText;

  const content = text
    ? `把下面的内容翻译成简体中文，保留原有的段落和列表格式：\n\n${text}`
    : [
        { type: "text", text: "把截图中红圈标记处的文字翻译成简体中文，只翻译红圈所在的那一段。" },
        { type: "image_url", image_url: { url: await api.screenshot() } },
      ];

  const result = await api.llm({
    system: "你是专业翻译。只输出译文，不要解释。原文已经是中文时，翻译成英文。",
    messages: [{ role: "user", content }],
  });
  await api.show("翻译", result);
}
