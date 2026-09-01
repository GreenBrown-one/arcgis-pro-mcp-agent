import type { BridgeSnapshot, ProviderSnapshot } from "../domain";

type SidebarProps = {
  provider: ProviderSnapshot;
  arcgis: BridgeSnapshot;
};

function statusLabel(arcgis: BridgeSnapshot) {
  if (arcgis.status === "error") return "连接异常";
  return arcgis.isLive ? "ArcGIS Pro 已连接" : "等待 ArcGIS Pro";
}

export function Sidebar({ provider, arcgis }: SidebarProps) {
  const label = provider.auth.status === "ready" ? provider.auth.label : null;
  const plan = provider.auth.status === "ready" ? provider.auth.plan : null;

  return (
    <nav className="sidebar" aria-label="主导航">
      <div className="sidebar-brand">
        <div className="brand-mark" aria-hidden="true">
          <span />
          <span />
          <span />
        </div>
        <div>
          <strong>ArcGIS Pro</strong>
          <span>智能助手</span>
        </div>
      </div>

      <div className="nav-section">
        <p className="nav-section-label">工作区</p>
        <button className="nav-item nav-item--active" type="button">
          <span aria-hidden="true">◫</span>
          新对话
        </button>
      </div>

      <div className="sidebar-spacer" />

      <section className="connection-card" aria-label="连接状态">
        <div className="connection-card-header">
          <span className={`status-dot status-dot--${arcgis.status}`} />
          <strong>{statusLabel(arcgis)}</strong>
        </div>
        <p>{arcgis.projectName || "尚未检测到活动项目"}</p>
        <div className="connection-meta">
          <span>ChatGPT / Codex</span>
          <span>{provider.runtime.status === "ready" ? "就绪" : "启动中"}</span>
        </div>
      </section>

      <section className="account-card" aria-label="当前账号">
        <span className="account-avatar" aria-hidden="true">
          {(label?.[0] || "C").toUpperCase()}
        </span>
        <span className="account-copy">
          <strong>{label || "ChatGPT 账号"}</strong>
          <small>{plan ? `${plan} 方案` : "已登录"}</small>
        </span>
        <span className="account-menu" aria-hidden="true">•••</span>
      </section>
    </nav>
  );
}
