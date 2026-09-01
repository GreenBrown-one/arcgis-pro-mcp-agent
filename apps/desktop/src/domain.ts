export type ProviderKind = "codex" | "deepSeek";

export type ProviderAuthSnapshot =
  | { status: "checking" }
  | { status: "needsSetup" }
  | { status: "loginPending"; loginId: string }
  | { status: "ready"; label: string | null; plan: string | null }
  | { status: "error"; code: string };

export type ProviderRuntimeSnapshot =
  | { status: "stopped" }
  | { status: "starting" }
  | { status: "ready"; version?: string | null; versionVerified?: boolean }
  | { status: "error"; code: string };

export type ProviderSnapshot = {
  kind: ProviderKind;
  auth: ProviderAuthSnapshot;
  runtime: ProviderRuntimeSnapshot;
};

export type ArcGisInstallSource =
  | "saved"
  | "registry"
  | "standard"
  | "compatible"
  | "manual";

export type ArcGisInstallation = {
  root: string;
  executable: string;
  version: string | null;
  source: ArcGisInstallSource;
};

export type ArcGisInstallSnapshot =
  | { status: "checking" }
  | { status: "ready"; installation: ArcGisInstallation }
  | { status: "notFound" }
  | { status: "error"; code: string };

export type AddInInstallerOpenResult = {
  packageName: string;
  requiresRestart: boolean;
};

export type BridgeSnapshot = {
  status: "connected" | "disconnected" | "error";
  isLive: boolean;
  contextIsLive?: boolean;
  protocolVersion?: string;
  addInVersion?: string;
  arcGisProVersion?: string;
  projectName?: string | null;
  projectHasUnsavedChanges?: boolean | null;
  activeMapName?: string | null;
  activeView?: ArcGisActiveView | null;
  layers?: ArcGisLayerSummary[];
  lastUpdated: string | null;
  error?: string;
};

export type ArcGisContextExtent = {
  xMin: number;
  yMin: number;
  xMax: number;
  yMax: number;
  wkid?: number | null;
};

export type ArcGisActiveView = {
  uri: string;
  name: string;
  kind: "map" | "scene" | "layout";
  extent?: ArcGisContextExtent | null;
};

export type ArcGisLayerSummary = {
  uri: string;
  name: string;
  longName: string;
  layerType: string;
  parentUri?: string | null;
  depth: number;
  visible: boolean;
  isFeatureLayer: boolean;
};

export type DesktopSnapshot = {
  provider: ProviderSnapshot;
  arcgis: Omit<BridgeSnapshot, "isLive">;
  arcgisInstall?: ArcGisInstallSnapshot;
  sessionGeneration?: number;
};

export type McpToolCallItem = {
  type: "mcpToolCall";
  server: string;
  tool: string;
  risk: "R0" | "R1" | "unknown";
  outcome: "succeeded" | "failed" | "unknown";
  durationMs?: number;
  summary?: string;
  errorCode?: string;
};

export type ProviderEvent =
  | { type: "textDelta"; itemId: string; text: string }
  | { type: "toolCompleted"; item: McpToolCallItem }
  | { type: "turnCompleted"; turnId: string };

export type DesktopEvent =
  | { type: "snapshot"; snapshot: DesktopSnapshot }
  | ProviderEvent
  | {
      type: "mcpServer/elicitation/declined";
      request: ElicitationDeclinedItem;
    };

export type ElicitationDeclinedItem = {
  requestId: string | number | null;
  serverName: "arcgis";
  threadId: string;
  message: string;
  mode: "form" | "url" | "unknown";
  outcome: "declined";
};

export type ConversationMessage = {
  id: string;
  role: "user" | "agent";
  text: string;
};

export type AppState = {
  provider: ProviderSnapshot;
  arcgis: BridgeSnapshot;
  arcgisInstall: ArcGisInstallSnapshot;
  inspectorOpen: boolean;
  sessionGeneration: number;
};

export type AppAction =
  | { type: "snapshot/received"; snapshot: DesktopSnapshot }
  | { type: "arcgisInstall/received"; snapshot: ArcGisInstallSnapshot }
  | { type: "bootstrap/failed"; error: string }
  | { type: "provider/loginCancelled" }
  | { type: "inspector/toggled" }
  | { type: "inspector/closed" };
