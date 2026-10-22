import React from "react";
import ReactDOM from "react-dom/client";
import "./styles.css";

const root = ReactDOM.createRoot(document.getElementById("root") as HTMLElement);
const inTauri = "__TAURI_INTERNALS__" in window;

async function boot() {
  if (!inTauri) {
    // 浏览器里打开 Vite 地址时进入预览模式，方便调界面
    document.documentElement.dataset.window = "preview";
    const view = new URLSearchParams(location.search).get("view");
    if (view === "toast") {
      const { default: Toast } = await import("./Toast");
      document.documentElement.dataset.window = "toast";
      const samples = [
        { icon: "ask", title: "问号", detail: "截图并让 AI 解释（功能开发中）", status: "pending" },
        { icon: "link", title: "打开", detail: "已打开 https://github.com", status: "done" },
        { icon: "link", title: "打开", detail: "还没有设置要打开的目标，请在菜单里填写", status: "failed" },
      ] as const;
      let i = 0;
      root.render(
        <Toast
          source={(show) => {
            show({ ...samples[0] });
            const t = setInterval(() => show({ ...samples[++i % samples.length] }), 1500);
            return () => clearInterval(t);
          }}
        />,
      );
      return;
    }
    if (view === "main" || view === "chat") {
      const { installPreviewMocks } = await import("./previewMocks");
      installPreviewMocks(view);
      document.documentElement.dataset.window = view;
      if (view === "chat") {
        const { default: ChatWindow } = await import("./ChatWindow");
        root.render(<ChatWindow />);
        const { emit } = await import("@tauri-apps/api/event");
        setTimeout(() => emit("chat:open", 1), 100);
      } else {
        const { default: MainWindow } = await import("./MainWindow");
        root.render(<MainWindow />);
      }
      return;
    }
    if (view === "editor") {
      const [{ default: WheelEditor }, { DEFAULT_CONFIG }] = await Promise.all([
        import("./WheelEditor"),
        import("./types"),
      ]);
      root.render(
        <main className="main">
          <WheelEditor
            initial={DEFAULT_CONFIG.items}
            onSave={async () => {}}
            onReset={async () => DEFAULT_CONFIG.items}
          />
        </main>,
      );
      return;
    }
    const { default: Preview } = await import("./Preview");
    root.render(<Preview />);
    return;
  }

  const { getCurrentWindow } = await import("@tauri-apps/api/window");
  const label = getCurrentWindow().label;
  document.documentElement.dataset.window = label;

  if (label === "toast") {
    const [{ default: Toast }, { listen }] = await Promise.all([
      import("./Toast"),
      import("@tauri-apps/api/event"),
    ]);
    root.render(
      <Toast
        source={(show, hide) => {
          const offs = [
            listen<import("./Toast").ToastData>("toast:show", (e) => show(e.payload)),
            listen("toast:hide", () => hide()),
          ];
          return () => offs.forEach((p) => p.then((f) => f()));
        }}
      />,
    );
  } else if (label === "chat") {
    const { default: ChatWindow } = await import("./ChatWindow");
    root.render(<ChatWindow />);
  } else if (label === "wheel") {
    const { default: WheelWindow } = await import("./WheelWindow");
    root.render(<WheelWindow />);
  } else {
    const { default: MainWindow } = await import("./MainWindow");
    root.render(
      <React.StrictMode>
        <MainWindow />
      </React.StrictMode>,
    );
  }
}

boot();
