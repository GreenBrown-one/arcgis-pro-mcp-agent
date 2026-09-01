import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useReducer,
  useRef,
  useState,
} from "react";
import { initialState, reduceAppState } from "./appStore";
import {
  cancelChatGptLogin,
  discoverArcGis,
  getSnapshot,
  launchArcGis,
  openAddinInstaller,
  openExternalUrl,
  rediscoverCodex,
  selectArcGisExecutable,
  startConversation,
  startChatGptLogin,
  startTurn,
  subscribeDesktopEvents,
} from "./desktopApi";
import type {
  ConversationMessage,
  DesktopEvent,
  DesktopSnapshot,
  ElicitationDeclinedItem,
  McpToolCallItem,
} from "./domain";
import { ArcGisContextPane } from "./components/ArcGisContextPane";
import { ConversationPane } from "./components/ConversationPane";
import { SetupView } from "./components/SetupView";
import { Sidebar } from "./components/Sidebar";
import "./app.css";

const narrowInspectorQuery = "(max-width: 979px)";
const maxToolCards = 100;
const maxToolTextCharacters = 200;
const r0Tools = new Set([
  "arcgis_capabilities",
  "arcgis_connection_status",
  "arcgis_count_features",
  "arcgis_describe_context",
  "arcgis_describe_layer",
  "arcgis_get_selection",
  "arcgis_list_fields",
  "arcgis_list_layers",
  "arcgis_query_features",
  "arcgis_query_spatial",
]);
const r1Tools = new Set([
  "arcgis_activate_view",
  "arcgis_clear_selection",
  "arcgis_flash_features",
  "arcgis_select_by_attribute",
  "arcgis_select_by_location",
  "arcgis_zoom_to_extent",
  "arcgis_zoom_to_layer",
]);

function boundedText(value: unknown, maxCharacters: number) {
  if (typeof value !== "string") return undefined;
  return Array.from(value).slice(0, maxCharacters).join("");
}

function safeToolCall(item: McpToolCallItem): McpToolCallItem {
  const candidate = boundedText(item.tool, 128) ?? "unknown";
  const tool = /^[a-z0-9_]+$/.test(candidate) ? candidate : "unknown";
  const risk = r0Tools.has(tool) ? "R0" : r1Tools.has(tool) ? "R1" : "unknown";
  const outcome = ["succeeded", "failed", "unknown"].includes(item.outcome)
    ? item.outcome
    : "unknown";
  const durationMs =
    typeof item.durationMs === "number" &&
    Number.isFinite(item.durationMs) &&
    item.durationMs >= 0 &&
    item.durationMs <= 86_400_000
      ? item.durationMs
      : undefined;
  const summary = boundedText(item.summary, maxToolTextCharacters);
  const errorCode =
    typeof item.errorCode === "string" && /^[a-z0-9_]{1,64}$/.test(item.errorCode)
      ? item.errorCode
      : undefined;
  return {
    type: "mcpToolCall",
    server: "arcgis",
    tool,
    risk,
    outcome,
    ...(durationMs === undefined ? {} : { durationMs }),
    ...(summary === undefined ? {} : { summary }),
    ...(errorCode === undefined ? {} : { errorCode }),
  };
}

function useNarrowInspector() {
  const [isNarrow, setIsNarrow] = useState(
    () => window.matchMedia(narrowInspectorQuery).matches,
  );

  useEffect(() => {
    const mediaQuery = window.matchMedia(narrowInspectorQuery);
    const updateLayout = (event: MediaQueryListEvent) => setIsNarrow(event.matches);

    setIsNarrow(mediaQuery.matches);
    mediaQuery.addEventListener("change", updateLayout);
    return () => mediaQuery.removeEventListener("change", updateLayout);
  }, []);

  return isNarrow;
}

const allowedAuthHosts = new Set([
  "auth.openai.com",
  "chatgpt.com",
  "openai.com",
]);

function isOfficialAuthUrl(value: string) {
  try {
    const url = new URL(value);
    return (
      url.protocol === "https:" &&
      url.username === "" &&
      url.password === "" &&
      url.port === "" &&
      allowedAuthHosts.has(url.hostname)
    );
  } catch {
    return false;
  }
}

function stateFromSnapshot(snapshot: DesktopSnapshot) {
  return reduceAppState(initialState, { type: "snapshot/received", snapshot });
}

type AppProps = {
  initialSnapshot?: DesktopSnapshot;
};

export function App({ initialSnapshot }: AppProps = {}) {
  const [state, dispatch] = useReducer(
    reduceAppState,
    initialSnapshot ? stateFromSnapshot(initialSnapshot) : initialState,
  );
  const [loginError, setLoginError] = useState<string>();
  const [composerText, setComposerText] = useState("");
  const [messages, setMessages] = useState<ConversationMessage[]>([]);
  const [toolCalls, setToolCalls] = useState<McpToolCallItem[]>([]);
  const [safetyRejections, setSafetyRejections] = useState<
    ElicitationDeclinedItem[]
  >([]);
  const [conversationError, setConversationError] = useState<string>();
  const [initializationError, setInitializationError] = useState<string>();
  const [setupActionError, setSetupActionError] = useState<string>();
  const [initializationAttempt, setInitializationAttempt] = useState(0);
  const [conversationReady, setConversationReady] = useState(false);
  const [conversationStartError, setConversationStartError] = useState<string>();
  const [sending, setSending] = useState(false);
  const isNarrow = useNarrowInspector();
  const inspectorToggleRef = useRef<HTMLButtonElement>(null);
  const inspectorCloseRef = useRef<HTMLButtonElement>(null);
  const restoreInspectorFocus = useRef(false);
  const conversationAttempt = useRef(0);
  const conversationStarting = useRef(false);
  const codexRediscovering = useRef(false);
  const sessionGeneration = useRef(initialSnapshot?.sessionGeneration ?? 0);

  const receiveSnapshot = useCallback((snapshot: DesktopSnapshot) => {
    const nextGeneration = snapshot.sessionGeneration ?? sessionGeneration.current;
    if (
      nextGeneration !== sessionGeneration.current ||
      snapshot.provider.auth.status !== "ready"
    ) {
      sessionGeneration.current = nextGeneration;
      conversationAttempt.current += 1;
      conversationStarting.current = false;
      setConversationReady(false);
      setConversationStartError(undefined);
      setComposerText("");
      setMessages([]);
      setToolCalls([]);
      setSafetyRejections([]);
      setConversationError(undefined);
    }
    setSetupActionError(undefined);
    dispatch({ type: "snapshot/received", snapshot });
  }, []);

  const receiveDesktopEvent = useCallback(
    (event: DesktopEvent | DesktopSnapshot) => {
      if (!("type" in event)) {
        receiveSnapshot(event);
      } else if (event.type === "snapshot") {
        receiveSnapshot(event.snapshot);
      } else if (event.type === "textDelta") {
        setMessages((current) => {
          const index = current.findIndex((message) => message.id === event.itemId);
          if (index === -1) {
            return [
              ...current,
              { id: event.itemId, role: "agent", text: event.text },
            ];
          }
          return current.map((message, messageIndex) =>
            messageIndex === index
              ? { ...message, text: message.text + event.text }
              : message,
          );
        });
      } else if (
        event.type === "toolCompleted" &&
        event.item.type === "mcpToolCall"
      ) {
        setToolCalls((current) =>
          [...current, safeToolCall(event.item)].slice(-maxToolCards),
        );
      } else if (event.type === "mcpServer/elicitation/declined") {
        setSafetyRejections((current) => [...current, event.request]);
      }
    },
    [receiveSnapshot],
  );

  useEffect(() => {
    let active = true;
    let failed = false;
    let ready = false;
    let unsubscribe: () => void = () => undefined;
    let bufferedSnapshot: DesktopSnapshot | undefined;

    const failInitialization = () => {
      if (!active || failed) return;
      failed = true;
      unsubscribe();
      unsubscribe = () => undefined;
      const error = "Codex 桌面连接失败，请重试。";
      setInitializationError(error);
      dispatch({ type: "bootstrap/failed", error });
    };

    const snapshotPromise =
      !initialSnapshot || initializationAttempt > 0
        ? getSnapshot()
        : Promise.resolve(initialSnapshot);
    const subscriptionPromise = subscribeDesktopEvents(
      (event: DesktopEvent | DesktopSnapshot) => {
        if (!active || failed) return;
        if (!ready) {
          if (!("type" in event)) bufferedSnapshot = event;
          else if (event.type === "snapshot") bufferedSnapshot = event.snapshot;
          return;
        }
        receiveDesktopEvent(event);
      },
    ).then((stop) => {
      if (!active || failed) {
        stop();
      } else {
        unsubscribe = stop;
      }
      return stop;
    });

    void Promise.all([snapshotPromise, subscriptionPromise])
      .then(([snapshot]) => {
        if (!active || failed) return;
        ready = true;
        setInitializationError(undefined);
        receiveSnapshot(bufferedSnapshot ?? snapshot);
      })
      .catch(failInitialization);

    return () => {
      active = false;
      unsubscribe();
      unsubscribe = () => undefined;
    };
  }, [
    initialSnapshot,
    initializationAttempt,
    receiveDesktopEvent,
    receiveSnapshot,
  ]);

  const beginConversation = useCallback(() => {
    if (conversationStarting.current) return;

    conversationStarting.current = true;
    const attempt = ++conversationAttempt.current;
    setConversationReady(false);
    setConversationStartError(undefined);
    void startConversation()
      .then(() => {
        if (conversationAttempt.current !== attempt) return;
        setConversationReady(true);
      })
      .catch(() => {
        if (conversationAttempt.current !== attempt) return;
        setConversationStartError("无法创建 ArcGIS 对话，请重试。");
      })
      .finally(() => {
        if (conversationAttempt.current === attempt) {
          conversationStarting.current = false;
        }
      });
  }, []);

  useEffect(() => {
    if (
      state.provider.kind === "codex" &&
      state.provider.auth.status === "ready" &&
      state.provider.runtime.status === "ready" &&
      !conversationReady &&
      !conversationStartError &&
      !conversationStarting.current
    ) {
      beginConversation();
    }
  }, [
    beginConversation,
    conversationReady,
    conversationStartError,
    state.provider.auth.status,
    state.provider.kind,
    state.provider.runtime.status,
  ]);

  useLayoutEffect(() => {
    if (!isNarrow) return;

    if (state.inspectorOpen) {
      inspectorCloseRef.current?.focus();
    } else if (restoreInspectorFocus.current) {
      inspectorToggleRef.current?.focus();
      restoreInspectorFocus.current = false;
    }
  }, [isNarrow, state.inspectorOpen]);

  useEffect(() => {
    if (!isNarrow || !state.inspectorOpen) return;

    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;

      event.preventDefault();
      restoreInspectorFocus.current = true;
      dispatch({ type: "inspector/closed" });
    };

    document.addEventListener("keydown", closeOnEscape);
    return () => document.removeEventListener("keydown", closeOnEscape);
  }, [isNarrow, state.inspectorOpen]);

  const handleLogin = () => {
    setLoginError(undefined);
    void (async () => {
      let loginPending = false;
      let failureMessage = "无法开始 ChatGPT 登录，请重试。";
      try {
        const login = await startChatGptLogin();
        loginPending = true;
        if (!isOfficialAuthUrl(login.authUrl)) {
          failureMessage = "登录地址未通过安全检查，请重试。";
          throw new Error("unsafe login URL");
        }
        await openExternalUrl(login.authUrl);
      } catch {
        if (loginPending) {
          await cancelChatGptLogin().catch(() => undefined);
          dispatch({ type: "provider/loginCancelled" });
        }
        setLoginError(failureMessage);
      }
    })();
  };

  const retryInitialization = () => {
    setInitializationAttempt((attempt) => attempt + 1);
  };

  const handleRediscoverCodex = useCallback(() => {
    if (initializationError) {
      retryInitialization();
      return;
    }
    if (codexRediscovering.current) return;

    codexRediscovering.current = true;
    setSetupActionError(undefined);
    void rediscoverCodex()
      .then(receiveSnapshot)
      .catch(() => setSetupActionError("无法重新检测 Codex CLI，请重试。"))
      .finally(() => {
        codexRediscovering.current = false;
      });
  }, [initializationError, receiveSnapshot]);

  const needsCodexRecheck =
    state.provider.kind === "codex" &&
    state.provider.runtime.status === "error" &&
    ["codex_not_found", "codex_incompatible"].includes(
      state.provider.runtime.code,
    );

  useEffect(() => {
    if (!needsCodexRecheck) return;

    const rediscoverOnFocus = () => handleRediscoverCodex();
    window.addEventListener("focus", rediscoverOnFocus);
    return () => window.removeEventListener("focus", rediscoverOnFocus);
  }, [handleRediscoverCodex, needsCodexRecheck]);

  const receiveArcGisInstall = useCallback((arcgisInstall: NonNullable<DesktopSnapshot["arcgisInstall"]>) => {
    setSetupActionError(undefined);
    dispatch({ type: "arcgisInstall/received", snapshot: arcgisInstall });
  }, []);

  const handleDiscoverArcGis = () => {
    setSetupActionError(undefined);
    void discoverArcGis()
      .then(receiveArcGisInstall)
      .catch(() => setSetupActionError("无法检测 ArcGIS Pro，请重试。"));
  };

  const handleSelectArcGis = () => {
    setSetupActionError(undefined);
    void selectArcGisExecutable()
      .then((arcgisInstall) => {
        if (arcgisInstall) receiveArcGisInstall(arcgisInstall);
      })
      .catch(() => setSetupActionError("无法验证所选 ArcGISPro.exe，请重试。"));
  };

  const handleOpenAddIn = async () => {
    setSetupActionError(undefined);
    try {
      return await openAddinInstaller();
    } catch {
      setSetupActionError("无法打开 Add-In 安装包，请重试。");
      throw new Error("无法打开 Add-In 安装包");
    }
  };

  const handleLaunchArcGis = () => {
    setSetupActionError(undefined);
    void launchArcGis().catch(() =>
      setSetupActionError("无法启动 ArcGIS Pro，请重试。"),
    );
  };

  const handleSend = () => {
    const message = composerText.trim();
    if (!message || sending || !conversationReady) return;

    const userMessage: ConversationMessage = {
      id: `user-${Date.now()}`,
      role: "user",
      text: message,
    };
    setConversationError(undefined);
    setSending(true);
    void startTurn(message)
      .then(() => {
        setMessages((current) => [...current, userMessage]);
        setComposerText("");
      })
      .catch(() => setConversationError("消息发送失败，请重试。"))
      .finally(() => setSending(false));
  };

  const toggleInspector = () => {
    if (isNarrow) restoreInspectorFocus.current = true;
    dispatch({ type: "inspector/toggled" });
  };

  const closeInspector = () => {
    if (isNarrow && state.inspectorOpen) restoreInspectorFocus.current = true;
    dispatch({ type: "inspector/closed" });
  };

  const bridgeReady = state.arcgis.status === "connected" && state.arcgis.isLive;
  const setupComplete =
    state.provider.kind === "codex" &&
    state.provider.auth.status === "ready" &&
    state.provider.runtime.status === "ready" &&
    state.arcgisInstall.status === "ready" &&
    bridgeReady &&
    conversationReady;

  if (!setupComplete) {
    return (
      <SetupView
        snapshot={{
          provider: state.provider,
          arcgis: state.arcgis,
          arcgisInstall: state.arcgisInstall,
          sessionGeneration: state.sessionGeneration,
        }}
        loginError={loginError}
        actionError={initializationError ?? setupActionError}
        conversationError={conversationStartError}
        onRediscoverCodex={handleRediscoverCodex}
        onDiscoverArcGis={handleDiscoverArcGis}
        onSelectArcGis={handleSelectArcGis}
        onOpenAddIn={handleOpenAddIn}
        onLogin={handleLogin}
        onLaunchArcGis={handleLaunchArcGis}
        onRetryConversation={beginConversation}
      />
    );
  }

  return (
    <div className="app-shell">
      <Sidebar provider={state.provider} arcgis={state.arcgis} />
      <ConversationPane
        arcgis={state.arcgis}
        inspectorOpen={state.inspectorOpen}
        showInspectorToggle={isNarrow}
        toggleButtonRef={inspectorToggleRef}
        onToggleInspector={toggleInspector}
        messages={messages}
        toolCalls={toolCalls}
        safetyRejections={safetyRejections}
        error={conversationStartError ?? conversationError}
        onRetry={
          conversationStartError
            ? beginConversation
            : conversationError
              ? handleSend
              : undefined
        }
        retryLabel={
          conversationStartError ? "重试创建会话" : "重试发送"
        }
        message={composerText}
        sending={sending}
        canSend={conversationReady}
        onMessageChange={setComposerText}
        onSend={handleSend}
      />
      <ArcGisContextPane
        arcgis={state.arcgis}
        drawer={isNarrow}
        hidden={isNarrow && !state.inspectorOpen}
        open={state.inspectorOpen}
        closeButtonRef={inspectorCloseRef}
        onClose={closeInspector}
      />
      <button
        className={`drawer-scrim ${state.inspectorOpen ? "drawer-scrim--open" : ""}`}
        type="button"
        aria-label="关闭 ArcGIS 上下文"
        aria-hidden="true"
        tabIndex={-1}
        onClick={closeInspector}
      />
    </div>
  );
}
