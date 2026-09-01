import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import { App } from "../src/App";
import type { DesktopSnapshot } from "../src/domain";
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

const signedOutSnapshot: DesktopSnapshot = {
  provider: {
    kind: "codex",
    auth: { status: "needsSetup" },
    runtime: { status: "ready", version: "0.144.5" },
  },
  arcgis: { status: "disconnected", lastUpdated: null },
};

let emit!: (event: import("../src/domain").DesktopEvent) => void;

function deferred<T>() {
  let resolve!: (value: T | PromiseLike<T>) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, resolve, reject };
}

const signedInSnapshot: DesktopSnapshot = {
  ...signedOutSnapshot,
  provider: {
    ...signedOutSnapshot.provider,
    auth: {
      status: "ready",
      label: "late@example.com",
      plan: "plus",
    },
  },
  arcgis: {
    status: "connected",
    contextIsLive: true,
    lastUpdated: "2026-08-27T00:00:00Z",
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
  sessionGeneration: 9,
};

beforeEach(() => {
  vi.resetAllMocks();
  vi.mocked(api.getSnapshot).mockResolvedValue(signedOutSnapshot);
  vi.mocked(api.subscribeDesktopEvents).mockImplementation(async (handler) => {
    emit = handler;
    return () => undefined;
  });
  vi.mocked(api.startConversation).mockResolvedValue({ threadId: "thread-1" });
  vi.mocked(api.cancelChatGptLogin).mockResolvedValue();
  Object.defineProperty(window, "matchMedia", {
    configurable: true,
    value: vi.fn().mockReturnValue({
      matches: false,
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
    }),
  });
});

it("opens only the official ChatGPT auth URL", async () => {
  vi.mocked(api.startChatGptLogin).mockResolvedValue({
    loginId: "login-1",
    authUrl: "https://auth.openai.com/oauth/authorize?client=codex",
  });

  render(<App />);
  fireEvent.click(
    await screen.findByRole("button", { name: "使用 ChatGPT 账号登录" }),
  );

  await waitFor(() =>
    expect(api.openExternalUrl).toHaveBeenCalledWith(
      "https://auth.openai.com/oauth/authorize?client=codex",
    ),
  );
});

it("does not open a URL whose host only resembles an official host", async () => {
  vi.mocked(api.startChatGptLogin).mockResolvedValue({
    loginId: "login-2",
    authUrl: "https://auth.openai.com.evil.example/oauth/authorize",
  });

  render(<App />);
  fireEvent.click(
    await screen.findByRole("button", { name: "使用 ChatGPT 账号登录" }),
  );

  expect(
    await screen.findByText("登录地址未通过安全检查，请重试。"),
  ).toBeVisible();
  expect(api.openExternalUrl).not.toHaveBeenCalled();
});

it("explains unsupported API-key auth and never starts a thread", async () => {
  vi.mocked(api.getSnapshot).mockResolvedValue({
    ...signedOutSnapshot,
    provider: {
      ...signedOutSnapshot.provider,
      auth: { status: "error", code: "unsupportedAuth" },
    },
  });

  render(<App />);

  expect(
    await screen.findByText(
      "当前登录方式不受首版支持，请退出后使用 ChatGPT 登录",
    ),
  ).toBeVisible();
  expect(api.startConversation).not.toHaveBeenCalled();
});

it("rolls back a pending login when the system opener fails and allows retry", async () => {
  vi.mocked(api.startChatGptLogin).mockImplementation(async () => {
    emit({
      type: "snapshot",
      snapshot: {
        ...signedOutSnapshot,
        provider: {
          ...signedOutSnapshot.provider,
          auth: { status: "loginPending", loginId: "login-retry" },
        },
        sessionGeneration: 1,
      },
    });
    return {
      loginId: "login-retry",
      authUrl: "https://auth.openai.com/oauth/authorize?client=codex",
    };
  });
  vi.mocked(api.openExternalUrl).mockRejectedValue(new Error("opener unavailable"));

  render(<App />);
  const login = await screen.findByRole("button", { name: /ChatGPT/ });
  fireEvent.click(login);

  expect(await screen.findByRole("alert")).toBeVisible();
  await waitFor(() => expect(api.cancelChatGptLogin).toHaveBeenCalledOnce());
  expect(login).toBeEnabled();

  fireEvent.click(login);
  await waitFor(() => expect(api.startChatGptLogin).toHaveBeenCalledTimes(2));
});

it("turns a snapshot bootstrap failure into a visible retryable Codex error", async () => {
  vi.mocked(api.getSnapshot)
    .mockRejectedValueOnce(new Error("snapshot unavailable"))
    .mockResolvedValueOnce(signedOutSnapshot);

  render(<App />);

  expect(await screen.findByRole("alert")).toHaveTextContent(/Codex/);
  fireEvent.click(await screen.findByRole("button", { name: /Codex/ }));

  await waitFor(() => expect(api.getSnapshot).toHaveBeenCalledTimes(2));
  expect(await screen.findByRole("button", { name: /ChatGPT/ })).toBeEnabled();
});

it("turns an event-listener bootstrap failure into a visible retryable Codex error", async () => {
  vi.mocked(api.subscribeDesktopEvents)
    .mockRejectedValueOnce(new Error("listen unavailable"))
    .mockImplementationOnce(async (handler) => {
      emit = handler;
      return () => undefined;
    });

  render(<App />);

  expect(await screen.findByRole("alert")).toHaveTextContent(/Codex/);
  fireEvent.click(await screen.findByRole("button", { name: /Codex/ }));

  await waitFor(() => expect(api.subscribeDesktopEvents).toHaveBeenCalledTimes(2));
  expect(await screen.findByRole("button", { name: /ChatGPT/ })).toBeEnabled();
});

it("ignores a late signed-in snapshot after the same bootstrap attempt loses its listener", async () => {
  const snapshot = deferred<DesktopSnapshot>();
  vi.mocked(api.getSnapshot).mockReturnValueOnce(snapshot.promise);
  vi.mocked(api.subscribeDesktopEvents).mockRejectedValueOnce(
    new Error("listener failed first"),
  );

  render(<App />);
  expect(await screen.findByRole("alert")).toHaveTextContent(/Codex/);

  await act(async () => snapshot.resolve(signedInSnapshot));

  expect(screen.getByRole("alert")).toHaveTextContent(/Codex/);
  expect(screen.queryByRole("textbox", { name: /ArcGIS/ })).not.toBeInTheDocument();
});

it("stops a listener that resolves after its snapshot attempt already failed", async () => {
  const subscription = deferred<() => void>();
  const stop = vi.fn();
  vi.mocked(api.getSnapshot).mockRejectedValueOnce(
    new Error("snapshot failed first"),
  );
  vi.mocked(api.subscribeDesktopEvents).mockReturnValueOnce(subscription.promise);

  render(<App />);
  expect(await screen.findByRole("alert")).toHaveTextContent(/Codex/);

  await act(async () => subscription.resolve(stop));

  expect(stop).toHaveBeenCalledOnce();
  expect(screen.getByRole("alert")).toHaveTextContent(/Codex/);
});

it("creates a fresh bootstrap attempt and enters the shell only after retry fully succeeds", async () => {
  const firstSubscription = deferred<() => void>();
  const firstStop = vi.fn();
  const retrySnapshot = deferred<DesktopSnapshot>();
  const retrySubscription = deferred<() => void>();
  const retryStop = vi.fn();
  vi.mocked(api.getSnapshot)
    .mockRejectedValueOnce(new Error("first snapshot failed"))
    .mockReturnValueOnce(retrySnapshot.promise);
  vi.mocked(api.subscribeDesktopEvents)
    .mockReturnValueOnce(firstSubscription.promise)
    .mockReturnValueOnce(retrySubscription.promise);

  render(<App />);
  expect(await screen.findByRole("alert")).toHaveTextContent(/Codex/);
  fireEvent.click(screen.getByRole("button", { name: /Codex/ }));

  await act(async () => firstSubscription.resolve(firstStop));
  expect(firstStop).toHaveBeenCalledOnce();
  expect(screen.getByRole("alert")).toBeVisible();

  await act(async () => retrySnapshot.resolve(signedInSnapshot));
  expect(screen.getByRole("alert")).toBeVisible();
  expect(screen.queryByRole("textbox", { name: /ArcGIS/ })).not.toBeInTheDocument();

  await act(async () => retrySubscription.resolve(retryStop));
  expect(await screen.findByRole("textbox", { name: /ArcGIS/ })).toBeEnabled();
  expect(firstStop).toHaveBeenCalledOnce();
  expect(retryStop).not.toHaveBeenCalled();
});
