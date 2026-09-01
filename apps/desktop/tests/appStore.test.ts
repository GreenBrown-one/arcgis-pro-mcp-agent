import { describe, expect, it } from "vitest";
import { initialState, reduceAppState } from "../src/appStore";

describe("app state", () => {
  it("marks a stale disconnected snapshot as non-live", () => {
    const state = reduceAppState(initialState, {
      type: "snapshot/received",
      snapshot: {
        provider: {
          kind: "codex",
          auth: { status: "needsSetup" },
          runtime: { status: "ready", version: "0.144.5" },
        },
        arcgis: {
          status: "disconnected",
          projectName: "过期项目",
          activeMapName: "过期地图",
          lastUpdated: "2026-07-19T00:00:00Z",
        },
      },
    });

    expect(state.arcgis.isLive).toBe(false);
    expect(state.arcgis.projectName).toBe("过期项目");
  });

  it("derives live state only from a connected snapshot", () => {
    const state = reduceAppState(initialState, {
      type: "snapshot/received",
      snapshot: {
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
          protocolVersion: "1.0",
          lastUpdated: "2026-07-19T00:00:00Z",
        },
      },
    });

    expect(state.arcgis.isLive).toBe(true);
  });

  it("toggles the inspector open and closed", () => {
    const open = reduceAppState(initialState, { type: "inspector/toggled" });
    const closed = reduceAppState(open, { type: "inspector/toggled" });

    expect(open.inspectorOpen).toBe(true);
    expect(closed.inspectorOpen).toBe(false);
  });

  it("closes an open inspector", () => {
    const state = reduceAppState(
      { ...initialState, inspectorOpen: true },
      { type: "inspector/closed" },
    );

    expect(state.inspectorOpen).toBe(false);
  });
});
