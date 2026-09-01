import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import { App } from "../src/App";
import { formatUpdatedAt } from "../src/components/ArcGisContextPane";
import type { DesktopEvent, DesktopSnapshot } from "../src/domain";
import * as api from "../src/desktopApi";

vi.mock("../src/desktopApi", () => ({
  getSnapshot: vi.fn(),
  rediscoverCodex: vi.fn(),
  discoverArcGis: vi.fn(),
  selectArcGisExecutable: vi.fn(),
  openAddinInstaller: vi.fn(),
  launchArcGis: vi.fn(),
  startChatGptLogin: vi.fn(),
  openExternalUrl: vi.fn(),
  startConversation: vi.fn(),
  startTurn: vi.fn(),
  interruptTurn: vi.fn(),
  logoutChatGpt: vi.fn(),
  subscribeDesktopEvents: vi.fn(),
}));

const signedInConnectedSnapshot: DesktopSnapshot = {
  provider: {
    kind: "codex",
    auth: {
      status: "ready",
      label: "user@example.com",
      plan: "plus",
    },
    runtime: { status: "ready", version: "0.144.5" },
  },
  arcgis: {
    status: "connected",
    contextIsLive: true,
    protocolVersion: "1.0",
    addInVersion: "0.1.0",
    arcGisProVersion: "3.5.2",
    projectName: "城市规划",
    projectHasUnsavedChanges: false,
    activeMapName: "中心城区",
    activeView: {
      uri: "map://central",
      name: "中心城区",
      kind: "map",
      extent: null,
    },
    layers: [],
    lastUpdated: "2026-07-19T00:00:00Z",
  },
  arcgisInstall: {
    status: "ready",
    installation: {
      root: "C:\\Program Files\\ArcGIS\\Pro",
      executable: "C:\\Program Files\\ArcGIS\\Pro\\bin\\ArcGISPro.exe",
      version: "3.7.1",
      source: "standard",
    },
  },
};

let emit!: (event: DesktopEvent) => void;

beforeEach(() => {
  vi.resetAllMocks();
  vi.mocked(api.getSnapshot).mockResolvedValue(signedInConnectedSnapshot);
  vi.mocked(api.startConversation).mockResolvedValue({ threadId: "thread-1" });
  vi.mocked(api.startTurn).mockResolvedValue({ turnId: "turn-1" });
  vi.mocked(api.subscribeDesktopEvents).mockImplementation(async (handler) => {
    emit = handler;
    return () => undefined;
  });
  Object.defineProperty(window, "matchMedia", {
    configurable: true,
    value: vi.fn().mockReturnValue({
      matches: false,
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
    }),
  });
});

it("sends text then renders streamed agent text and ArcGIS MCP cards", async () => {
  render(<App initialSnapshot={signedInConnectedSnapshot} />);
  const input = await screen.findByRole("textbox", { name: "ArcGIS 指令" });
  fireEvent.change(input, { target: { value: "检查 ArcGIS 连接" } });
  fireEvent.click(screen.getByRole("button", { name: "发送" }));

  await waitFor(() =>
    expect(api.startTurn).toHaveBeenCalledWith("检查 ArcGIS 连接"),
  );
  act(() => {
    emit({
      type: "textDelta",
      itemId: "agent-1",
      text: "连接",
    });
    emit({
      type: "textDelta",
      itemId: "agent-1",
      text: "正常",
    });
    emit({
      type: "toolCompleted",
      item: {
        type: "mcpToolCall",
        server: "arcgis",
        tool: "arcgis_connection_status",
        risk: "R0",
        outcome: "succeeded",
        durationMs: 42,
        summary: "completed=true",
      },
    });
  });

  expect(await screen.findByText("连接正常")).toBeVisible();
  const card = screen.getByRole("article", {
    name: "ArcGIS 工具 arcgis_connection_status",
  });
  expect(card).toHaveTextContent("arcgis");
  expect(card).toHaveTextContent("连接状态");
  expect(card).toHaveTextContent("R0");
  expect(card).toHaveTextContent("succeeded");
  expect(card).toHaveTextContent("completed=true");
  expect(card).toHaveTextContent("42 ms");
});

it("renders model text literally instead of interpreting raw HTML", async () => {
  render(<App initialSnapshot={signedInConnectedSnapshot} />);
  await screen.findByRole("textbox", { name: "ArcGIS 指令" });

  act(() => {
    emit({
      type: "textDelta",
      itemId: "agent-html",
      text: '<img src=x onerror="window.__unsafe=true">',
    });
  });

  expect(
    await screen.findByText('<img src=x onerror="window.__unsafe=true">'),
  ).toBeVisible();
  expect(document.querySelector("img")).toBeNull();
});

it("returns to setup after a failed Bridge health refresh without generic expiry copy", async () => {
  render(<App initialSnapshot={signedInConnectedSnapshot} />);
  await screen.findByText("实时连接");

  act(() => {
    emit({
      type: "snapshot",
      snapshot: {
        ...signedInConnectedSnapshot,
        arcgis: {
          ...signedInConnectedSnapshot.arcgis,
          status: "disconnected",
          contextIsLive: false,
          error: "ArcGIS 连接检查失败",
        },
      },
    });
  });

  expect(await screen.findByText("等待 ArcGIS Add-In 连接")).toBeVisible();
  expect(screen.queryByText("ArcGIS 连接已过期")).not.toBeInTheDocument();
});

it("labels connected health with stale context as context-expired", async () => {
  const view = render(
    <App
      initialSnapshot={{
        ...signedInConnectedSnapshot,
        arcgis: {
          ...signedInConnectedSnapshot.arcgis,
          status: "connected",
          contextIsLive: false,
        },
      }}
    />,
  );
  await screen.findByRole("textbox", { name: "ArcGIS 指令" });

  expect(view.container.querySelector(".live-pill")).toHaveTextContent(
    "ArcGIS Pro 尚未连接",
  );
  expect(view.container.querySelector(".live-pill")).not.toHaveClass(
    "live-pill--ready",
  );
});

it("stops the desktop event subscription when the app unmounts", async () => {
  const stop = vi.fn();
  vi.mocked(api.subscribeDesktopEvents).mockImplementation(async (handler) => {
    emit = handler;
    return stop;
  });

  const view = render(<App initialSnapshot={signedInConnectedSnapshot} />);
  await screen.findByRole("textbox", { name: "ArcGIS 指令" });
  view.unmount();

  expect(stop).toHaveBeenCalledOnce();
});

it("shows a retryable turn failure without a false user message", async () => {
  vi.mocked(api.startTurn)
    .mockRejectedValueOnce(new Error("turn rejected"))
    .mockResolvedValueOnce({ turnId: "turn-retry" });
  render(<App initialSnapshot={signedInConnectedSnapshot} />);
  const input = await screen.findByRole("textbox", { name: /ArcGIS/ });
  fireEvent.change(input, { target: { value: "retry this turn" } });
  fireEvent.click(screen.getByRole("button", { name: "发送" }));

  expect(await screen.findByRole("alert")).toBeVisible();
  expect(input).toHaveValue("retry this turn");
  expect(document.querySelector(".message--user")).toBeNull();

  fireEvent.click(screen.getByRole("button", { name: "重试发送" }));
  await waitFor(() => expect(api.startTurn).toHaveBeenCalledTimes(2));
  await waitFor(() =>
    expect(document.querySelector(".message--user")).toHaveTextContent(
      "retry this turn",
    ),
  );
  expect(input).toHaveValue("");
});

it("resets conversation artifacts when the account session generation changes", async () => {
  const firstSession = { ...signedInConnectedSnapshot, sessionGeneration: 10 };
  render(<App initialSnapshot={firstSession} />);
  await screen.findByRole("textbox", { name: /ArcGIS/ });

  act(() => {
    emit({ type: "textDelta", itemId: "old-agent", text: "old answer" });
    emit({
      type: "toolCompleted",
      item: {
        type: "mcpToolCall",
        server: "arcgis",
        tool: "old_tool",
        risk: "unknown",
        outcome: "succeeded",
      },
    });
    emit({
      type: "mcpServer/elicitation/declined",
      request: {
        requestId: 7,
        serverName: "arcgis",
        threadId: "thread-old",
        message: "old approval",
        mode: "form",
        outcome: "declined",
      },
    });
  });
  expect(await screen.findByText("old answer")).toBeVisible();
  expect(screen.getByText("old_tool")).toBeVisible();

  act(() => {
    emit({
      type: "snapshot",
      snapshot: { ...firstSession, sessionGeneration: 11 },
    });
  });

  await waitFor(() => expect(screen.queryByText("old answer")).not.toBeInTheDocument());
  expect(screen.queryByText("old_tool")).not.toBeInTheDocument();
  expect(screen.queryByText("old approval")).not.toBeInTheDocument();
});

it("renders a structured ArcGIS elicitation decline card", async () => {
  render(<App initialSnapshot={signedInConnectedSnapshot} />);
  await screen.findByRole("textbox", { name: /ArcGIS/ });

  act(() => {
    emit({
      type: "mcpServer/elicitation/declined",
      request: {
        requestId: "approval-1",
        serverName: "arcgis",
        threadId: "thread-1",
        message: "Delete selected features?",
        mode: "form",
        outcome: "declined",
      },
    });
  });

  const card = await screen.findByRole("article", { name: /ArcGIS/ });
  expect(card).toHaveTextContent("Delete selected features?");
  expect(card).toHaveTextContent("declined");
});

it("renders structured risk, failure code, and aggregate summary literally", async () => {
  render(<App initialSnapshot={signedInConnectedSnapshot} />);
  await screen.findByRole("textbox", { name: /ArcGIS/ });

  act(() => {
    emit({
      type: "toolCompleted",
      item: {
        type: "mcpToolCall",
        server: "arcgis",
        tool: "arcgis_select_by_location",
        risk: "R1",
        outcome: "failed",
        durationMs: 18,
        summary: '<img src=x onerror="window.__unsafe=true">',
        errorCode: "no_active_view",
      },
    });
  });

  const card = await screen.findByRole("article", {
    name: "ArcGIS 工具 arcgis_select_by_location",
  });
  expect(card).toHaveTextContent("按位置选择");
  expect(card).toHaveTextContent("R1");
  expect(card).toHaveTextContent("failed");
  expect(card).toHaveTextContent("18 ms");
  expect(card).toHaveTextContent('<img src=x onerror="window.__unsafe=true">');
  expect(card).toHaveTextContent("no_active_view");
  expect(document.querySelector("img")).toBeNull();
});

it("keeps only the latest 100 structured tool cards", async () => {
  render(<App initialSnapshot={signedInConnectedSnapshot} />);
  await screen.findByRole("textbox", { name: /ArcGIS/ });

  act(() => {
    for (let index = 0; index < 101; index += 1) {
      emit({
        type: "toolCompleted",
        item: {
          type: "mcpToolCall",
          server: "arcgis",
          tool: `arcgis_unknown_${index}`,
          risk: "unknown",
          outcome: "unknown",
        },
      });
    }
  });

  await waitFor(() =>
    expect(screen.getAllByRole("article", { name: /ArcGIS 工具/ })).toHaveLength(100),
  );
  expect(screen.queryByText("arcgis_unknown_0")).not.toBeInTheDocument();
  expect(screen.getByText("arcgis_unknown_100")).toBeVisible();
});

it("parses RFC3339 last-updated values before displaying them", () => {
  const value = "2026-07-19T12:34:56Z";
  const expected = new Intl.DateTimeFormat("zh-CN", {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  }).format(new Date(value));

  expect(formatUpdatedAt(value)).toBe(expected);
  expect(formatUpdatedAt(value)).not.toBe(value);
});

it("keeps the conversation shell hidden after creation fails and retries to success", async () => {
  vi.mocked(api.startConversation)
    .mockRejectedValueOnce(new Error("thread rejected"))
    .mockResolvedValueOnce({ threadId: "thread-recovered" });
  render(<App initialSnapshot={signedInConnectedSnapshot} />);

  expect(await screen.findByRole("alert")).toBeVisible();
  expect(screen.getByRole("main", { name: "便捷设置" })).toBeVisible();
  expect(screen.queryByRole("textbox", { name: /ArcGIS/ })).not.toBeInTheDocument();
  expect(api.startTurn).not.toHaveBeenCalled();

  fireEvent.click(screen.getByRole("button", { name: "重试创建 ArcGIS 对话" }));
  await waitFor(() => expect(api.startConversation).toHaveBeenCalledTimes(2));
  await waitFor(() =>
    expect(screen.getByRole("textbox", { name: /ArcGIS/ })).toBeEnabled(),
  );
  expect(screen.queryByRole("alert")).not.toBeInTheDocument();

  const recoveredInput = screen.getByRole("textbox", { name: /ArcGIS/ });
  fireEvent.change(recoveredInput, { target: { value: "send after recovery" } });
  fireEvent.click(screen.getByRole("button", { name: "发送" }));
  await waitFor(() =>
    expect(api.startTurn).toHaveBeenCalledWith("send after recovery"),
  );
});
