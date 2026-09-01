import type { ProviderAuthSnapshot } from "../domain";

type LoginViewProps = {
  auth: ProviderAuthSnapshot;
  error?: string;
  onLogin: () => void;
  retryingInitialization?: boolean;
};

export function LoginView({
  auth,
  error,
  onLogin,
  retryingInitialization,
}: LoginViewProps) {
  const isPending = auth.status === "loginPending";
  const unsupported = auth.status === "error" && auth.code === "unsupportedAuth";

  return (
    <main className="login-view">
      <section className="login-card" aria-labelledby="login-title">
        <div className="brand-mark brand-mark--large" aria-hidden="true">
          <span />
          <span />
          <span />
        </div>
        <p className="eyebrow">CHATGPT / CODEX</p>
        <h1 id="login-title">ArcGIS Pro 智能助手</h1>
        <p className="login-copy">
          在一个安全、专注的工作区中连接 ChatGPT 与当前 ArcGIS Pro 项目。
        </p>
        {unsupported ? (
          <p className="login-error" role="alert">
            当前登录方式不受首版支持，请退出后使用 ChatGPT 登录
          </p>
        ) : null}
        {error ? <p className="login-error" role="alert">{error}</p> : null}
        <button
          className="primary-button"
          type="button"
          aria-label={
            retryingInitialization
              ? "重试 Codex 连接"
              : "使用 ChatGPT 账号登录"
          }
          disabled={isPending || unsupported}
          onClick={onLogin}
        >
          <span className="chatgpt-glyph" aria-hidden="true">✦</span>
          {retryingInitialization
            ? "重试连接"
            : isPending
              ? "正在打开登录窗口…"
              : "使用 ChatGPT 账号登录"}
        </button>
        <p className="login-footnote">通过 ChatGPT 账号安全授权。</p>
      </section>
    </main>
  );
}
