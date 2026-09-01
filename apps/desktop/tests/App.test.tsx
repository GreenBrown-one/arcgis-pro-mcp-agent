import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { App } from "../src/App";
import type { DesktopSnapshot } from "../src/domain";
import {
  getSnapshot,
  startConversation,
  startChatGptLogin,
  subscribeDesktopEvents,
} from "../src/desktopApi";

vi.mock("../src/desktopApi", () => ({
  getSnapshot: vi.fn(),
  rediscoverCodex: vi.fn(),
  discoverArcGis: vi.fn(),
  selectArcGisExecutable: vi.fn(),
  openAddinInstaller: vi.fn(),
  launchArcGis: vi.fn(),
  startChatGptLogin: vi.fn(),
  openExternalUrl: vi.fn(),
  startConversation: vi.fn().mockResolvedValue({ threadId: "thread-1" }),
  startTurn: vi.fn(),
  interruptTurn: vi.fn(),
  logoutChatGpt: vi.fn(),
  subscribeDesktopEvents: vi.fn(),
}));

const mockedGetSnapshot = vi.mocked(getSnapshot);
const mockedStartChatGptLogin = vi.mocked(startChatGptLogin);
const mockedStartConversation = vi.mocked(startConversation);
const mockedSubscribeDesktopEvents = vi.mocked(subscribeDesktopEvents);

const signedOutSnapshot: DesktopSnapshot = {
  provider: {
    kind: "codex",
    auth: { status: "needsSetup" },
    runtime: { status: "ready", version: "0.144.5" },
  },
  arcgis: { status: "disconnected", lastUpdated: null },
};

const signedInSnapshot: DesktopSnapshot = {
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
    projectName: "城市规划",
    projectHasUnsavedChanges: true,
    activeMapName: "中心城区",
    activeView: {
      uri: "map://central",
      name: "中心城区",
      kind: "map",
      extent: { xMin: 1.25, yMin: 2.5, xMax: 9.75, yMax: 10, wkid: 4326 },
    },
    layers: [
      {
        uri: "layer://infrastructure",
        name: "基础设施",
        longName: "基础设施",
        layerType: "GroupLayer",
        parentUri: null,
        depth: 0,
        visible: true,
        isFeatureLayer: false,
      },
      {
        uri: "layer://roads",
        name: "道路",
        longName: "基础设施\\道路",
        layerType: "FeatureLayer",
        parentUri: "layer://infrastructure",
        depth: 1,
        visible: false,
        isFeatureLayer: true,
      },
    ],
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

function installMatchMedia(matches: boolean) {
  const mediaQueryList = {
    matches,
    media: "(max-width: 979px)",
    onchange: null,
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
    addListener: vi.fn(),
    removeListener: vi.fn(),
    dispatchEvent: vi.fn(),
  } as unknown as MediaQueryList;

  Object.defineProperty(window, "matchMedia", {
    configurable: true,
    writable: true,
    value: vi.fn().mockReturnValue(mediaQueryList),
  });
}

async function renderSignedIn() {
  mockedGetSnapshot.mockResolvedValue(signedInSnapshot);
  const view = render(<App />);
  await screen.findByRole("main", { name: "对话" });
  return view;
}

beforeEach(() => {
  vi.resetAllMocks();
  installMatchMedia(true);
  mockedStartChatGptLogin.mockResolvedValue({
    loginId: "login-1",
    authUrl: "https://auth.openai.com/oauth/authorize",
  });
  mockedStartConversation.mockResolvedValue({ threadId: "thread-1" });
  mockedSubscribeDesktopEvents.mockResolvedValue(() => undefined);
});

afterEach(() => {
  vi.restoreAllMocks();
});

it("shows ChatGPT login without API-key controls", async () => {
  mockedGetSnapshot.mockResolvedValue(signedOutSnapshot);

  render(<App />);

  const loginButton = await screen.findByRole("button", {
    name: "使用 ChatGPT 账号登录",
  });
  expect(loginButton).toBeVisible();
  expect(screen.queryByText(/API Key/i)).not.toBeInTheDocument();

  fireEvent.click(loginButton);
  await waitFor(() => expect(mockedStartChatGptLogin).toHaveBeenCalledOnce());
});

it("shows the signed-in shell with its ArcGIS prompt", async () => {
  await renderSignedIn();

  expect(screen.getByRole("navigation", { name: "主导航" })).toBeVisible();
  expect(screen.getByRole("main", { name: "对话" })).toBeVisible();

  const prompt = screen.getByPlaceholderText("输入 ArcGIS 指令");
  await waitFor(() => expect(prompt).toBeEnabled());
  expect(screen.queryByText(/API Key/i)).not.toBeInTheDocument();
});

it("opens the narrow inspector, handles Escape, and restores toggle focus", async () => {
  await renderSignedIn();

  const toggle = screen.getByRole("button", {
    name: "切换 ArcGIS 上下文",
    hidden: true,
  });
  const pane = document.getElementById("arcgis-context-pane");

  expect(toggle).toHaveAttribute("aria-controls", "arcgis-context-pane");
  expect(toggle).toHaveAttribute("aria-expanded", "false");
  expect(pane).toHaveAttribute("aria-hidden", "true");
  expect(pane).toHaveAttribute("inert");
  expect(
    screen.queryByRole("dialog", { name: "ArcGIS 上下文" }),
  ).not.toBeInTheDocument();

  fireEvent.click(toggle);

  const dialog = await screen.findByRole("dialog", { name: "ArcGIS 上下文" });
  const close = within(dialog).getByRole("button", {
    name: "关闭 ArcGIS 上下文",
    hidden: true,
  });
  await waitFor(() => expect(close).toHaveFocus());
  expect(toggle).toHaveAttribute("aria-expanded", "true");
  expect(dialog).not.toHaveAttribute("aria-hidden");
  expect(dialog).not.toHaveAttribute("inert");

  fireEvent.keyDown(document, { key: "Escape" });

  await waitFor(() => expect(toggle).toHaveFocus());
  expect(toggle).toHaveAttribute("aria-expanded", "false");
  expect(pane).toHaveAttribute("aria-hidden", "true");

  fireEvent.click(toggle);
  await waitFor(() => expect(close).toHaveFocus());
  fireEvent.click(close);
  await waitFor(() => expect(toggle).toHaveFocus());
});

it("keeps the desktop inspector visible and hides its drawer toggle", async () => {
  installMatchMedia(false);
  await renderSignedIn();

  expect(
    screen.getByRole("complementary", { name: "ArcGIS 上下文" }),
  ).toBeVisible();
  const toggle = document.querySelector<HTMLButtonElement>(".context-toggle");
  expect(toggle).toHaveAttribute("hidden");
  expect(toggle).not.toBeVisible();
});

it("renders live dirty view context, finite extent, and a collapsible nested layer tree", async () => {
  installMatchMedia(false);
  await renderSignedIn();

  const context = screen.getByRole("complementary", { name: "ArcGIS 上下文" });
  expect(within(context).getByText("实时连接")).toBeVisible();
  expect(within(context).getByText("城市规划")).toBeVisible();
  expect(within(context).getByText("未保存")).toBeVisible();
  expect(within(context).getByText(/中心城区.*地图/)).toBeVisible();
  expect(within(context).getByText(/1\.25.*2\.50.*9\.75.*10\.00/)).toBeVisible();
  expect(within(context).getByText(/WKID 4326/)).toBeVisible();
  expect(within(context).getByText("2 个图层")).toBeVisible();

  const details = within(context).getByText("图层摘要").closest("details");
  expect(details).toHaveAttribute("open");
  const child = within(context).getByText("道路").closest("li");
  expect(child).toHaveAttribute("data-depth", "1");
  expect(child).toHaveTextContent("不可见");
  expect(child).toHaveTextContent("FeatureLayer");

  fireEvent.click(within(context).getByText("图层摘要"));
  expect(details).not.toHaveAttribute("open");
});

it("runs an immediately available subscription cleanup on unmount", async () => {
  const stop = vi.fn();
  mockedSubscribeDesktopEvents.mockResolvedValue(stop);

  const view = await renderSignedIn();
  await waitFor(() => expect(mockedSubscribeDesktopEvents).toHaveBeenCalledOnce());
  view.unmount();

  await waitFor(() => expect(stop).toHaveBeenCalledOnce());
});

it("runs a deferred subscription cleanup when it resolves after unmount", async () => {
  let resolveSubscription!: (stop: () => void) => void;
  const stop = vi.fn();
  mockedSubscribeDesktopEvents.mockReturnValue(
    new Promise((resolve) => {
      resolveSubscription = resolve;
    }),
  );

  mockedGetSnapshot.mockResolvedValue(signedInSnapshot);
  const view = render(<App />);
  await waitFor(() => expect(mockedSubscribeDesktopEvents).toHaveBeenCalledOnce());
  view.unmount();

  await act(async () => resolveSubscription(stop));

  expect(stop).toHaveBeenCalledOnce();
});

it("ignores desktop events after unmount without a React warning", async () => {
  const consoleError = vi.spyOn(console, "error").mockImplementation(() => undefined);
  const view = await renderSignedIn();
  await waitFor(() => expect(mockedSubscribeDesktopEvents).toHaveBeenCalledOnce());
  const eventHandler = mockedSubscribeDesktopEvents.mock.calls[0][0];

  view.unmount();
  await act(async () =>
    eventHandler({ type: "snapshot", snapshot: signedOutSnapshot }),
  );

  expect(consoleError).not.toHaveBeenCalled();
});
