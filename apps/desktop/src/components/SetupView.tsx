import { useEffect, useState, type ReactNode } from "react";
import type {
  AddInInstallerOpenResult,
  ArcGisInstallSnapshot,
  DesktopSnapshot,
  ProviderKind,
  ProviderRuntimeSnapshot,
} from "../domain";
import { openExternalUrl } from "../desktopApi";

export type SetupViewProps = {
  snapshot: DesktopSnapshot;
  loginError?: string;
  actionError?: string;
  onRediscoverCodex: () => void;
  onDiscoverArcGis: () => void;
  onSelectArcGis: () => void;
  onOpenAddIn: () => Promise<AddInInstallerOpenResult>;
  onLogin: () => void;
  onLaunchArcGis: () => void;
  conversationError?: string;
  onRetryConversation: () => void;
};

function codexLabel(runtime: ProviderRuntimeSnapshot, kind: ProviderKind) {
  if (kind !== "codex") return "ChatGPT / Codex 不可用";
  if (runtime.status === "starting") return "正在检测 Codex CLI";
  if (runtime.status === "error" && runtime.code === "codex_not_found") {
    return "未找到 Codex CLI";
  }
  if (runtime.status === "error" && runtime.code === "codex_incompatible") {
    return "Codex 与当前版本不兼容";
  }
  if (runtime.status === "ready" && !runtime.versionVerified) {
    return `Codex ${runtime.version ?? "未知版本"} 未经本版验证`;
  }
  if (runtime.status === "ready") return "Codex CLI 已就绪";
  return "Codex CLI 不可用，请重新检测";
}

function arcGisLabel(install: ArcGisInstallSnapshot) {
  if (install.status === "checking") return "正在检测 ArcGIS Pro 3.7";
  if (install.status === "ready") return "ArcGIS Pro 3.7 已就绪";
  if (install.status === "notFound") return "未找到 ArcGIS Pro 3.7";
  return "ArcGIS Pro 3.7 不可用";
}

function SetupCard({
  title,
  label,
  complete,
  children,
}: {
  title: string;
  label: string;
  complete: boolean;
  children?: ReactNode;
}) {
  return (
    <details className="setup-card" open={!complete}>
      <summary>
        <span>{title}</span>
        <strong className={complete ? "setup-card-status setup-card-status--ready" : "setup-card-status"}>
          {label}
        </strong>
      </summary>
      {children ? <div className="setup-card-body">{children}</div> : null}
    </details>
  );
}

export function SetupView({
  snapshot,
  loginError,
  actionError,
  conversationError,
  onRediscoverCodex,
  onDiscoverArcGis,
  onSelectArcGis,
  onOpenAddIn,
  onLogin,
  onLaunchArcGis,
  onRetryConversation,
}: SetupViewProps) {
  const [addinOpened, setAddinOpened] = useState(false);
  const runtime = snapshot.provider.runtime;
  const install = snapshot.arcgisInstall ?? { status: "checking" as const };
  const codexReady = snapshot.provider.kind === "codex" && runtime.status === "ready";
  const arcGisReady = install.status === "ready";
  const chatGptReady = snapshot.provider.auth.status === "ready";
  const bridgeConnected = snapshot.arcgis.status === "connected";
  const loginPending = snapshot.provider.auth.status === "loginPending";
  const unsupportedAuth =
    snapshot.provider.auth.status === "error" &&
    snapshot.provider.auth.code === "unsupportedAuth";

  useEffect(() => {
    if (bridgeConnected) setAddinOpened(false);
  }, [bridgeConnected]);

  return (
    <main className="setup-view" aria-label="便捷设置">
      <section className="setup-panel" aria-labelledby="setup-title">
        <p className="eyebrow">QUICK SETUP</p>
        <h1 id="setup-title">连接 ArcGIS Pro 智能助手</h1>
        <p className="setup-copy">完成以下检查后，即可使用 ChatGPT 协助当前 ArcGIS Pro 项目。</p>
        {actionError || loginError || conversationError ? (
          <p className="login-error" role="alert">
            {actionError ?? loginError ?? conversationError}
          </p>
        ) : null}

        <div className="setup-cards">
          <SetupCard
            title="Codex CLI"
            label={codexLabel(runtime, snapshot.provider.kind)}
            complete={codexReady}
          >
            {snapshot.provider.kind !== "codex" || runtime.status === "error" || runtime.status === "stopped" ? (
              <>
                <p>请安装或更新 Codex CLI 后重新检测。</p>
                <button type="button" className="secondary-button" onClick={() => void onRediscoverCodex()}>
                  重新检测 Codex CLI
                </button>
                <button
                  type="button"
                  className="link-button"
                  onClick={() => void openExternalUrl("https://learn.chatgpt.com/docs/codex/cli")}
                >
                  查看官方安装说明
                </button>
              </>
            ) : runtime.status === "starting" ? (
              <p>正在检查本机的 Codex CLI。</p>
            ) : (
              <p>Codex CLI 已可用于 ChatGPT 登录和 ArcGIS 工具发现。</p>
            )}
          </SetupCard>

          <SetupCard title="ArcGIS Pro 3.7" label={arcGisLabel(install)} complete={arcGisReady}>
            {arcGisReady ? (
              <p>{install.installation.executable}</p>
            ) : (
              <>
                <p>自动检测未找到兼容的 ArcGIS Pro 3.7 安装。</p>
                <div className="setup-actions">
                  <button type="button" className="secondary-button" onClick={() => void onDiscoverArcGis()}>
                    重新检测 ArcGIS Pro
                  </button>
                  <button type="button" className="secondary-button" onClick={() => void onSelectArcGis()}>
                    选择 ArcGISPro.exe
                  </button>
                </div>
              </>
            )}
          </SetupCard>

          <SetupCard
            title="ArcGIS Add-In"
            label={bridgeConnected ? "ArcGIS Add-In 已连接" : "等待 ArcGIS Add-In 连接"}
            complete={bridgeConnected}
          >
            {addinOpened ? (
              <p>安装包已打开。请重启 ArcGIS Pro，然后返回此窗口等待连接。</p>
            ) : (
              <p>安装并启用随应用提供的 ArcGIS Add-In。</p>
            )}
            <button
              type="button"
              className="secondary-button"
              onClick={() => {
                void onOpenAddIn()
                  .then((result) => setAddinOpened(result.requiresRestart))
                  .catch(() => undefined);
              }}
            >
              打开 Add-In 安装包
            </button>
          </SetupCard>

          <SetupCard
            title="ChatGPT"
            label={chatGptReady ? "ChatGPT 已登录" : loginPending ? "正在等待 ChatGPT 登录" : "需要 ChatGPT 登录"}
            complete={chatGptReady}
          >
            {unsupportedAuth ? (
              <p>当前登录方式不受首版支持，请退出后使用 ChatGPT 登录</p>
            ) : (
              <>
                <p>通过 ChatGPT 账号安全授权。</p>
                <button
                  type="button"
                  className="primary-button"
                  disabled={!codexReady || loginPending}
                  onClick={() => void onLogin()}
                >
                  {loginPending ? "正在打开登录窗口…" : "使用 ChatGPT 账号登录"}
                </button>
              </>
            )}
          </SetupCard>

          <SetupCard
            title="启动与自检"
            label={
              conversationError
                ? "ArcGIS 对话创建失败"
                : bridgeConnected
                  ? "ArcGIS Pro 已连接"
                  : "等待启动和连接"
            }
            complete={bridgeConnected && !conversationError}
          >
            {conversationError ? (
              <>
                <p>ArcGIS 对话创建失败，请重新执行自检。</p>
                <button
                  type="button"
                  className="primary-button"
                  onClick={() => void onRetryConversation()}
                >
                  重试创建 ArcGIS 对话
                </button>
              </>
            ) : (
              <>
                <p>启动 ArcGIS Pro 后，助手会自动检查 Add-In Bridge 连接。</p>
                <button
                  type="button"
                  className="primary-button"
                  disabled={!codexReady || !arcGisReady || !chatGptReady}
                  onClick={() => void onLaunchArcGis()}
                >
                  启动 ArcGIS Pro 并连接
                </button>
              </>
            )}
          </SetupCard>
        </div>
      </section>
    </main>
  );
}
