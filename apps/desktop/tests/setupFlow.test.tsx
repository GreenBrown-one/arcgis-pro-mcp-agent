import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import { App } from "../src/App";
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
  cancelChatGptLogin: vi.fn(),
  openExternalUrl: vi.fn(),
  startConversation: vi.fn(),
  startTurn: vi.fn(),
  interruptTurn: vi.fn(),
  logoutChatGpt: vi.fn(),
  subscribeDesktopEvents: vi.fn(),
}));

const readyArcGisInstall = {
  status: "ready" as const,
  installation: {
    root: "C:\\Program Files\\ArcGIS\\Pro",
    executable: "C:\\Program Files\\ArcGIS\\Pro\\bin\\ArcGISPro.exe",
    version: "3.7.1",
    source: "manual" as const,
  },
};

const missingCodexSnapshot: DesktopSnapshot = {
  provider: {
    kind: "codex",
    auth: { status: "needsSetup" },
    runtime: { status: "error", code: "codex_not_found" },
  },
  arcgis: { status: "disconnected", lastUpdated: null },
  arcgisInstall: { status: "notFound" },
};

const unverifiedSignedOutSnapshot: DesktopSnapshot = {
  ...missingCodexSnapshot,
  provider: {
    kind: "codex",
    auth: { status: "needsSetup" },
    runtime: { status: "ready", version: "0.150.1", versionVerified: false },
  },
  arcgisInstall: readyArcGisInstall,
};

const arcGisMissingSnapshot: DesktopSnapshot = {
  ...unverifiedSignedOutSnapshot,
  provider: {
    ...unverifiedSignedOutSnapshot.provider,
    runtime: { status: "ready", version: "0.150.1", versionVerified: true },
  },
  arcgisInstall: { status: "notFound" },
};

const readyExceptBridgeSnapshot: DesktopSnapshot = {
  ...unverifiedSignedOutSnapshot,
  provider: {
    ...unverifiedSignedOutSnapshot.provider,
    auth: { status: "ready", label: "map@example.com", plan: "plus" },
    runtime: { status: "ready", version: "0.150.1", versionVerified: true },
  },
  arcgisInstall: readyArcGisInstall,
};

const fullyReadySnapshot: DesktopSnapshot = {
  ...readyExceptBridgeSnapshot,
  arcgis: {
    status: "connected",
    contextIsLive: true,
    protocolVersion: "1.0",
    projectName: "城市规划",
    lastUpdated: "2026-08-27T00:00:00Z",
  },
};

let emitSnapshot: (snapshot: DesktopSnapshot) => void;

beforeEach(() => {
  vi.resetAllMocks();
  vi.mocked(api.getSnapshot).mockResolvedValue(missingCodexSnapshot);
  vi.mocked(api.rediscoverCodex).mockResolvedValue(missingCodexSnapshot);
  vi.mocked(api.discoverArcGis).mockResolvedValue({ status: "notFound" });
  vi.mocked(api.openAddinInstaller).mockResolvedValue({
    packageName: "ArcGISProAgent.esriAddInX",
    requiresRestart: true,
  });
  vi.mocked(api.launchArcGis).mockResolvedValue(1234);
  vi.mocked(api.startConversation).mockResolvedValue({ threadId: "thread-1" });
  vi.mocked(api.subscribeDesktopEvents).mockImplementation(async (handler) => {
    emitSnapshot = (snapshot) => handler({ type: "snapshot", snapshot });
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

it("shows only ChatGPT and guides a missing Codex user", async () => {
  render(<App initialSnapshot={missingCodexSnapshot} />);

  expect(await screen.findByText("未找到 Codex CLI")).toBeVisible();
  fireEvent.click(screen.getByRole("button", { name: "查看官方安装说明" }));
  expect(api.openExternalUrl).toHaveBeenCalledWith(
    "https://learn.chatgpt.com/docs/codex/cli",
  );
  expect(screen.queryByText(/DeepSeek/i)).not.toBeInTheDocument();
  expect(screen.queryByLabelText(/API Key/i)).not.toBeInTheDocument();
});

it("redetects Codex when the window regains focus", async () => {
  render(<App initialSnapshot={missingCodexSnapshot} />);
  await screen.findByText("未找到 Codex CLI");
  fireEvent.focus(window);
  await waitFor(() => expect(api.rediscoverCodex).toHaveBeenCalledOnce());
});

it("does not rediscover Codex while initial detection is already running", async () => {
  render(
    <App
      initialSnapshot={{
        ...missingCodexSnapshot,
        provider: {
          ...missingCodexSnapshot.provider,
          runtime: { status: "starting" },
        },
      }}
    />,
  );

  await screen.findByText("正在检测 Codex CLI");
  fireEvent.focus(window);
  expect(api.rediscoverCodex).not.toHaveBeenCalled();
});

it("removes Codex focus rediscovery when an eligible error becomes ineligible", async () => {
  render(<App initialSnapshot={missingCodexSnapshot} />);
  await screen.findByText("未找到 Codex CLI");

  act(() =>
    emitSnapshot({
      ...missingCodexSnapshot,
      provider: {
        ...missingCodexSnapshot.provider,
        runtime: { status: "error", code: "provider_unavailable" },
      },
    }),
  );

  fireEvent.focus(window);
  expect(api.rediscoverCodex).not.toHaveBeenCalled();
});

it("allows an unverified compatible version and continues to ChatGPT login", async () => {
  render(<App initialSnapshot={unverifiedSignedOutSnapshot} />);
  expect(await screen.findByText("Codex 0.150.1 未经本版验证")).toBeVisible();
  expect(
    screen.getByRole("button", { name: "使用 ChatGPT 账号登录" }),
  ).toBeEnabled();
});

it("fails closed for a non-Codex provider snapshot", async () => {
  render(
    <App
      initialSnapshot={{
        ...fullyReadySnapshot,
        provider: { ...fullyReadySnapshot.provider, kind: "deepSeek" },
      }}
    />,
  );

  expect(await screen.findByRole("main", { name: "便捷设置" })).toBeVisible();
  expect(screen.getByText("ChatGPT / Codex 不可用")).toBeVisible();
  expect(screen.queryByRole("main", { name: "对话" })).not.toBeInTheDocument();
  expect(api.startConversation).not.toHaveBeenCalled();
});

it.each([
  { status: "stopped" as const },
  { status: "error" as const, code: "provider_unavailable" },
])("labels unavailable Codex runtime %# for recheck", async (runtime) => {
  render(
    <App
      initialSnapshot={{
        ...missingCodexSnapshot,
        provider: { ...missingCodexSnapshot.provider, runtime },
      }}
    />,
  );

  expect(await screen.findByText("Codex CLI 不可用，请重新检测")).toBeVisible();
  expect(screen.getByRole("button", { name: "重新检测 Codex CLI" })).toBeEnabled();
});

it("offers a validated ArcGIS executable picker after automatic discovery fails", async () => {
  vi.mocked(api.selectArcGisExecutable).mockResolvedValue(readyArcGisInstall);
  render(<App initialSnapshot={arcGisMissingSnapshot} />);
  fireEvent.click(await screen.findByRole("button", { name: "选择 ArcGISPro.exe" }));
  await waitFor(() => expect(api.selectArcGisExecutable).toHaveBeenCalledOnce());
  expect(await screen.findByText("ArcGIS Pro 3.7 已就绪")).toBeVisible();
});

it("collapses completed Codex, ArcGIS, and ChatGPT cards after readiness transitions", async () => {
  render(<App initialSnapshot={readyExceptBridgeSnapshot} />);
  await waitFor(() => expect(api.subscribeDesktopEvents).toHaveBeenCalledOnce());

  for (const title of ["Codex CLI", "ArcGIS Pro 3.7", "ChatGPT"]) {
    expect(screen.getByText(title).closest("details")).not.toHaveAttribute("open");
  }

  act(() => emitSnapshot({ ...readyExceptBridgeSnapshot, arcgisInstall: { status: "notFound" } }));
  expect(screen.getByText("ArcGIS Pro 3.7").closest("details")).toHaveAttribute("open");

  act(() => emitSnapshot(readyExceptBridgeSnapshot));
  expect(screen.getByText("ArcGIS Pro 3.7").closest("details")).not.toHaveAttribute("open");
});

it("keeps ArcGIS selection retryable when the file dialog is cancelled", async () => {
  vi.mocked(api.selectArcGisExecutable).mockResolvedValue(null);
  render(<App initialSnapshot={arcGisMissingSnapshot} />);
  const select = await screen.findByRole("button", { name: "选择 ArcGISPro.exe" });
  fireEvent.click(select);
  await waitFor(() => expect(api.selectArcGisExecutable).toHaveBeenCalledOnce());
  expect(select).toBeEnabled();
  expect(screen.getByText("未找到 ArcGIS Pro 3.7")).toBeVisible();
});

it("shows Add-In restart guidance until the Bridge connects", async () => {
  render(<App initialSnapshot={readyExceptBridgeSnapshot} />);
  fireEvent.click(await screen.findByRole("button", { name: "打开 Add-In 安装包" }));
  expect(await screen.findByText(/请重启 ArcGIS Pro/)).toBeVisible();
  expect(screen.getByText("ArcGIS Add-In").closest("details")).toHaveAttribute("open");
});

it("shows Add-In restart guidance only when the package requires it", async () => {
  vi.mocked(api.openAddinInstaller).mockResolvedValue({
    packageName: "ArcGISProAgent.esriAddInX",
    requiresRestart: false,
  });
  render(<App initialSnapshot={readyExceptBridgeSnapshot} />);
  fireEvent.click(await screen.findByRole("button", { name: "打开 Add-In 安装包" }));
  await waitFor(() => expect(api.openAddinInstaller).toHaveBeenCalledOnce());
  expect(screen.queryByText(/请重启 ArcGIS Pro/)).not.toBeInTheDocument();
});

it("keeps failed setup actions retryable", async () => {
  vi.mocked(api.openAddinInstaller).mockRejectedValueOnce(new Error("open failed"));
  render(<App initialSnapshot={readyExceptBridgeSnapshot} />);
  const openAddIn = await screen.findByRole("button", { name: "打开 Add-In 安装包" });
  fireEvent.click(openAddIn);
  expect(await screen.findByRole("alert")).toBeVisible();
  expect(openAddIn).toBeEnabled();
  fireEvent.click(openAddIn);
  await waitFor(() => expect(api.openAddinInstaller).toHaveBeenCalledTimes(2));
});

it("launches ArcGIS from the single primary action and enters chat after connection", async () => {
  render(<App initialSnapshot={readyExceptBridgeSnapshot} />);
  fireEvent.click(
    await screen.findByRole("button", { name: "启动 ArcGIS Pro 并连接" }),
  );
  expect(api.launchArcGis).toHaveBeenCalledOnce();
  act(() => emitSnapshot(fullyReadySnapshot));
  expect(await screen.findByRole("main", { name: "对话" })).toBeVisible();
  expect(screen.queryByText("ArcGIS 连接已过期")).not.toBeInTheDocument();
});

it("keeps setup visible and retries conversation creation until it succeeds", async () => {
  vi.mocked(api.startConversation)
    .mockRejectedValueOnce(new Error("thread rejected"))
    .mockResolvedValueOnce({ threadId: "thread-recovered" });
  render(<App initialSnapshot={fullyReadySnapshot} />);

  expect(await screen.findByRole("main", { name: "便捷设置" })).toBeVisible();
  expect(screen.getByRole("alert")).toHaveTextContent("无法创建 ArcGIS 对话");
  expect(screen.queryByRole("main", { name: "对话" })).not.toBeInTheDocument();

  fireEvent.click(screen.getByRole("button", { name: "重试创建 ArcGIS 对话" }));
  await waitFor(() => expect(api.startConversation).toHaveBeenCalledTimes(2));
  expect(await screen.findByRole("main", { name: "对话" })).toBeVisible();
});
