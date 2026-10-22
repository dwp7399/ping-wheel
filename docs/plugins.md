# Ping Wheel 插件开发

插件可以把自定义动作挂到轮盘的某个方向上。使用自定义插件需要订阅「高级功能」。

## 包结构

插件是一个 zip 包，根目录（或包内唯一的一层目录）里有：

```
manifest.json   插件描述
main.js         入口，ES 模块
```

打包：

```bash
scripts/pack-plugin.sh examples/plugins/translate
```

在主窗口「插件」页选择 zip 安装；同 id 的插件再次安装即为升级。卸载会删除插件文件、插件存储，并把它从轮盘上移除。

## manifest.json

```json
{
  "id": "com.example.translate",
  "name": "标记翻译",
  "version": "1.0.0",
  "description": "把标记处的内容翻译成中文",
  "author": "你的名字",
  "permissions": ["scene", "screenshot", "llm"],
  "actions": [{ "id": "translate", "label": "翻译", "icon": "translate" }],
  "main": "main.js"
}
```

- `id`：小写字母、数字、点和横线，全局唯一
- `actions`：插件提供的动作，每个动作可以放到轮盘的一个方向上。`icon` 可选：`ask` `mark` `collect` `chat` `link` `translate` `note` `camera` `terminal` `sparkle` `folder` `code`
- `permissions`：插件要用的能力，没声明的调用会被拒绝

| 权限 | 能力 |
|---|---|
| `scene` | 读取当前应用、窗口标题、链接、文件，以及标记处 / 附近 / 选中的文字、网页正文 |
| `screenshot` | 截取当前屏幕（已用红圈标出唤出位置） |
| `llm` | 调用通用模型接口，消耗用户的模型额度（需要模型订阅） |
| `clipboard` | 写入剪贴板 |
| `open` | 打开 http(s) 链接 |

## main.js

导出 `run(ctx, api)`，可以是 async 函数。抛出的错误会以提示的形式显示给用户。每次运行最长 3 分钟。

```js
export async function run(ctx, api) {
  const text = ctx.scene?.selection || ctx.scene?.focusText;
  const out = await api.llm({
    system: "你是专业翻译，只输出译文。",
    messages: [{ role: "user", content: `翻译成中文：\n${text}` }],
  });
  await api.show("翻译", out);
}
```

### ctx

```ts
{
  action: string;                       // 被触发的动作 id
  plugin: { id, name, version };
  scene?: {                             // 需要 scene 权限
    app, title, url, file,
    selection, focusText, nearbyText, pageText
  };
}
```

### api

| 方法 | 说明 |
|---|---|
| `api.screenshot()` | 返回当前屏幕的 JPEG data URL（`screenshot` 权限） |
| `api.llm({ system?, messages, max_tokens? })` | 通用模型接口，返回正文。`messages` 为 `[{ role, content }]`，`content` 可以是字符串，也可以是 `[{type:"text",text}, {type:"image_url",image_url:{url}}]`（`llm` 权限） |
| `api.show(title, markdown)` | 在唤出点旁的悬浮面板显示结果，用户可以继续追问 |
| `api.toast(title, detail?, status?)` | 在唤出点弹提示，`status` 为 `done` / `pending` / `failed` |
| `api.copy(text)` | 写剪贴板（`clipboard` 权限） |
| `api.open(url)` | 打开 http(s) 链接（`open` 权限） |
| `api.storage.get(key)` / `api.storage.set(key, value)` | 插件自己的键值存储，上限 1MB |

## 运行环境与安全

插件代码在一个隐藏的独立窗口里运行，这个窗口只能调用插件 API；每个 API 再按 `permissions` 检查。插件拿不到用户的账号令牌，也不能读写标记、收藏等应用数据。
