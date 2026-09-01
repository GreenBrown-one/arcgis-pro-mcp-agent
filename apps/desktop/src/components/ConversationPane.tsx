import type { FormEvent, RefObject } from "react";
import type {
  BridgeSnapshot,
  ConversationMessage,
  ElicitationDeclinedItem,
  McpToolCallItem,
} from "../domain";

type ConversationPaneProps = {
  arcgis: BridgeSnapshot;
  inspectorOpen: boolean;
  showInspectorToggle: boolean;
  toggleButtonRef: RefObject<HTMLButtonElement | null>;
  onToggleInspector: () => void;
  messages: ConversationMessage[];
  toolCalls: McpToolCallItem[];
  safetyRejections: ElicitationDeclinedItem[];
  error?: string;
  onRetry?: () => void;
  retryLabel: string;
  message: string;
  sending: boolean;
  canSend: boolean;
  onMessageChange: (message: string) => void;
  onSend: () => void;
};

const toolLabels: Record<string, string> = {
  arcgis_activate_view: "激活视图",
  arcgis_capabilities: "能力列表",
  arcgis_clear_selection: "清除选择",
  arcgis_connection_status: "连接状态",
  arcgis_count_features: "要素计数",
  arcgis_describe_context: "描述上下文",
  arcgis_describe_layer: "描述图层",
  arcgis_flash_features: "闪烁要素",
  arcgis_get_selection: "选择摘要",
  arcgis_list_fields: "字段列表",
  arcgis_list_layers: "图层列表",
  arcgis_query_features: "查询要素",
  arcgis_query_spatial: "空间查询",
  arcgis_select_by_attribute: "按属性选择",
  arcgis_select_by_location: "按位置选择",
  arcgis_zoom_to_extent: "缩放至范围",
  arcgis_zoom_to_layer: "缩放至图层",
};

export function ConversationPane({
  arcgis,
  inspectorOpen,
  showInspectorToggle,
  toggleButtonRef,
  onToggleInspector,
  messages,
  toolCalls,
  safetyRejections,
  error,
  onRetry,
  retryLabel,
  message,
  sending,
  canSend,
  onMessageChange,
  onSend,
}: ConversationPaneProps) {
  const contextIsLive = arcgis.contextIsLive ?? arcgis.isLive;
  const submit = (event: FormEvent) => {
    event.preventDefault();
    onSend();
  };

  return (
    <main className="conversation-pane" aria-label="对话">
      <header className="conversation-header">
        <div>
          <p className="eyebrow">ACTIVE SESSION</p>
          <h2>ArcGIS 工作对话</h2>
        </div>
        <button
          ref={toggleButtonRef}
          className="context-toggle"
          type="button"
          hidden={!showInspectorToggle}
          aria-label="切换 ArcGIS 上下文"
          aria-controls="arcgis-context-pane"
          aria-expanded={inspectorOpen}
          onClick={onToggleInspector}
        >
          <span aria-hidden="true">◎</span>
          上下文
        </button>
      </header>

      <section className="conversation-body" aria-live="polite">
        {error ? (
          <div className="context-error" role="alert">
            <span>{error}</span>
            {onRetry ? (
              <button type="button" aria-label={retryLabel} onClick={onRetry}>
                重试
              </button>
            ) : null}
          </div>
        ) : null}
        {messages.length === 0 && toolCalls.length === 0 && safetyRejections.length === 0 ? (
          <>
            <div className="welcome-orbit" aria-hidden="true">
              <span className="welcome-orbit-ring" />
              <span className="welcome-orbit-core">✦</span>
            </div>
            <p className="eyebrow">ARCGIS COPILOT</p>
            <h1>准备好探索你的地图</h1>
            <p className="welcome-copy">
              我可以帮助你理解项目上下文、规划操作并执行 ArcGIS 指令。
            </p>
            <div className={`live-pill ${contextIsLive ? "live-pill--ready" : ""}`}>
              <span />
              {contextIsLive ? "ArcGIS Pro 上下文在线" : "ArcGIS Pro 尚未连接"}
            </div>
          </>
        ) : (
          <div className="message-list">
            {messages.map((entry) => (
              <p key={entry.id} className={`message message--${entry.role}`}>
                {entry.text}
              </p>
            ))}
            {toolCalls.map((call, index) => (
              <article
                key={`${call.server}:${call.tool}:${index}`}
                className="tool-card"
                aria-label={`ArcGIS 工具 ${call.tool}`}
              >
                <header className="tool-card-header">
                  <strong>{toolLabels[call.tool] ?? "未知 ArcGIS 工具"}</strong>
                  <code>{call.tool}</code>
                </header>
                <div className="tool-card-meta">
                  <span className={`tool-risk tool-risk--${call.risk.toLowerCase()}`}>
                    {call.risk}
                  </span>
                  <span>结果：{call.outcome}</span>
                  {call.durationMs === undefined ? null : (
                    <span>{call.durationMs} ms</span>
                  )}
                </div>
                {call.summary ? (
                  <p className="tool-card-summary">{call.summary}</p>
                ) : null}
                {call.errorCode ? (
                  <p className="tool-card-error">错误代码：{call.errorCode}</p>
                ) : null}
              </article>
            ))}
            {safetyRejections.map((request, index) => (
              <article
                key={`${String(request.requestId)}:${index}`}
                className="tool-card"
                aria-label="ArcGIS 安全请求已拒绝"
              >
                <strong>安全请求已拒绝</strong>
                <span>{request.serverName}</span>
                <span>{request.message}</span>
                <span>{request.outcome}</span>
              </article>
            ))}
          </div>
        )}
      </section>

      <footer className="composer-wrap">
        <form className="composer" onSubmit={submit}>
          <textarea
            aria-label="ArcGIS 指令"
            placeholder="输入 ArcGIS 指令"
            value={message}
            disabled={sending || !canSend}
            onChange={(event) => onMessageChange(event.currentTarget.value)}
            rows={1}
          />
          <button
            type="submit"
            disabled={sending || !canSend || message.trim().length === 0}
            aria-label="发送"
          >
            ↑
          </button>
        </form>
        <p>模型与工具输出始终按纯文本和结构化字段显示。</p>
      </footer>
    </main>
  );
}
