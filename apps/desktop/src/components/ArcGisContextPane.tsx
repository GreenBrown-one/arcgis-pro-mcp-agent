import type { RefObject } from "react";
import type { BridgeSnapshot } from "../domain";

type ArcGisContextPaneProps = {
  arcgis: BridgeSnapshot;
  drawer: boolean;
  hidden: boolean;
  open: boolean;
  closeButtonRef: RefObject<HTMLButtonElement | null>;
  onClose: () => void;
};

function displayValue(value: string | null | undefined) {
  return value || "—";
}

function viewKindLabel(kind: "map" | "scene" | "layout") {
  return kind === "map" ? "地图" : kind === "scene" ? "场景" : "布局";
}

function formatExtent(arcgis: BridgeSnapshot) {
  const extent = arcgis.activeView?.extent;
  if (!extent) return "—";
  const coordinates = [extent.xMin, extent.yMin, extent.xMax, extent.yMax];
  if (!coordinates.every(Number.isFinite)) return "—";
  const wkid = extent.wkid == null ? "" : ` · WKID ${extent.wkid}`;
  return `${extent.xMin.toFixed(2)}, ${extent.yMin.toFixed(2)} — ${extent.xMax.toFixed(2)}, ${extent.yMax.toFixed(2)}${wkid}`;
}

export function formatUpdatedAt(value: string | null) {
  if (!value) return "尚未同步";

  const date = new Date(value);
  return Number.isNaN(date.getTime())
    ? value
    : new Intl.DateTimeFormat("zh-CN", {
        hour: "2-digit",
        minute: "2-digit",
        second: "2-digit",
      }).format(date);
}

export function ArcGisContextPane({
  arcgis,
  drawer,
  hidden,
  open,
  closeButtonRef,
  onClose,
}: ArcGisContextPaneProps) {
  const contextIsLive = arcgis.contextIsLive ?? arcgis.isLive;
  const layers = arcgis.layers ?? [];
  const hasRetainedContext = Boolean(
    arcgis.projectName || arcgis.activeView || layers.length || arcgis.lastUpdated,
  );
  const statusTitle = contextIsLive
    ? arcgis.status === "connected"
      ? "实时连接"
      : "当前未连接"
    : hasRetainedContext
      ? "连接已过期"
      : "未连接";
  const statusCopy = contextIsLive
    ? arcgis.status === "connected"
      ? "上下文已同步"
      : "ArcGIS Pro 当前未提供项目上下文"
    : hasRetainedContext
      ? "显示最近一次成功同步的上下文"
      : "正在等待 ArcGIS Pro Add-In";
  const activeView = arcgis.activeView;

  return (
    <aside
      id="arcgis-context-pane"
      className={`context-pane ${open ? "context-pane--open" : ""} ${!contextIsLive && hasRetainedContext ? "context-pane--stale" : ""}`}
      role={drawer ? "dialog" : "complementary"}
      aria-label="ArcGIS 上下文"
      aria-modal={drawer ? true : undefined}
      aria-hidden={hidden ? true : undefined}
      inert={hidden || undefined}
    >
      <header className="context-header">
        <div>
          <p className="eyebrow">LIVE CONTEXT</p>
          <h2>ArcGIS 上下文</h2>
        </div>
        <button
          ref={closeButtonRef}
          className="context-close"
          type="button"
          aria-label="关闭 ArcGIS 上下文"
          onClick={onClose}
        >
          ×
        </button>
      </header>

      <section className="context-status">
        <div className={`context-status-icon context-status-icon--${arcgis.status}`}>
          <span />
        </div>
        <div>
          <strong>{statusTitle}</strong>
          <p>{statusCopy}</p>
        </div>
      </section>

      {arcgis.error ? <p className="context-error">{arcgis.error}</p> : null}

      <section className="context-section">
        <div className="context-section-heading">
          <h3>当前工作区</h3>
          {!contextIsLive && hasRetainedContext ? (
            <span className="context-stale-marker">已过期</span>
          ) : null}
        </div>
        <dl>
          <div>
            <dt>项目</dt>
            <dd>
              {displayValue(arcgis.projectName)}
              {arcgis.projectHasUnsavedChanges ? (
                <span className="context-dirty-marker">未保存</span>
              ) : null}
            </dd>
          </div>
          <div>
            <dt>活动视图</dt>
            <dd>
              {activeView
                ? `${activeView.name} · ${viewKindLabel(activeView.kind)}`
                : displayValue(arcgis.activeMapName)}
            </dd>
          </div>
          <div>
            <dt>当前范围</dt>
            <dd className="context-extent">{formatExtent(arcgis)}</dd>
          </div>
          <div>
            <dt>图层</dt>
            <dd>{layers.length} 个图层</dd>
          </div>
        </dl>
      </section>

      <section className="context-section context-layer-section">
        <details open>
          <summary>图层摘要</summary>
          {layers.length === 0 ? (
            <p className="context-empty">当前活动地图没有可显示的图层摘要。</p>
          ) : (
            <ul className="context-layer-list">
              {layers.map((layer) => {
                const depth = Math.min(8, Math.max(0, Math.trunc(layer.depth)));
                return (
                  <li
                    key={layer.uri}
                    data-depth={depth}
                    style={{ paddingInlineStart: `${depth * 14}px` }}
                  >
                    <span className="context-layer-name">{layer.name}</span>
                    <span className="context-layer-meta">
                      {layer.visible ? "可见" : "不可见"} · {layer.layerType}
                      {layer.isFeatureLayer ? " · 要素图层" : ""}
                    </span>
                  </li>
                );
              })}
            </ul>
          )}
        </details>
      </section>

      <section className="context-section">
        <h3>连接详情</h3>
        <dl>
          <div>
            <dt>协议版本</dt>
            <dd>{displayValue(arcgis.protocolVersion)}</dd>
          </div>
          <div>
            <dt>Add-In</dt>
            <dd>{displayValue(arcgis.addInVersion)}</dd>
          </div>
          <div>
            <dt>ArcGIS Pro</dt>
            <dd>{displayValue(arcgis.arcGisProVersion)}</dd>
          </div>
        </dl>
      </section>

      <p className="last-updated">最后同步：{formatUpdatedAt(arcgis.lastUpdated)}</p>
    </aside>
  );
}
