import type { AppAction, AppState } from "./domain";

export const initialState: AppState = {
  provider: {
    kind: "codex",
    auth: { status: "checking" },
    runtime: { status: "starting" },
  },
  arcgis: {
    status: "disconnected",
    isLive: false,
    lastUpdated: null,
  },
  arcgisInstall: { status: "checking" },
  inspectorOpen: false,
  sessionGeneration: 0,
};

function assertNever(value: never): never {
  throw new Error(`Unhandled app action: ${JSON.stringify(value)}`);
}

export function reduceAppState(state: AppState, action: AppAction): AppState {
  switch (action.type) {
    case "snapshot/received":
      return {
        ...state,
        provider: action.snapshot.provider,
        arcgis: {
          ...action.snapshot.arcgis,
          isLive: action.snapshot.arcgis.status === "connected",
        },
        arcgisInstall: action.snapshot.arcgisInstall ?? state.arcgisInstall,
        sessionGeneration:
          action.snapshot.sessionGeneration ?? state.sessionGeneration,
      };
    case "bootstrap/failed":
      return {
        ...state,
        provider: {
          ...state.provider,
          auth: { status: "needsSetup" },
          runtime: { status: "error", code: "providerUnavailable" },
        },
      };
    case "arcgisInstall/received":
      return { ...state, arcgisInstall: action.snapshot };
    case "provider/loginCancelled":
      return {
        ...state,
        provider: { ...state.provider, auth: { status: "needsSetup" } },
      };
    case "inspector/toggled":
      return { ...state, inspectorOpen: !state.inspectorOpen };
    case "inspector/closed":
      return { ...state, inspectorOpen: false };
    default:
      return assertNever(action);
  }
}
