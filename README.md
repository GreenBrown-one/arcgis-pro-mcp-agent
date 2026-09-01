# ArcGIS Pro 智能助手

ArcGIS Pro 智能助手是一套仅在当前 Windows 用户会话中运行的本机桌面应用。它通过官方 Codex App Server 使用 ChatGPT 订阅账号登录，以受约束的 MCP 工具连接 ArcGIS Pro Add-In；应用不会接收 API Key、复制登录令牌，也不会把 GIS 数据视为安装文件。

当前可交付的 Windows 预览版为 `0.2.0-preview.1`；它建立在 `0.1.0` Phase 2 的受约束读取、选择和导航切片之上，包含：

- Tauri + React 三栏桌面界面；
- ChatGPT 官方浏览器登录、对话和流式工具事件；
- 精确暴露 17 个 .NET MCP 工具：10 个 R0 连接/上下文/图层/字段/计数/查询工具和 7 个 R1 临时选择/导航工具；
- 协议 `1.0`、当前用户命名管道和启动令牌保护的 ArcGIS Pro 3.7 Add-In；
- 有界、脱敏的实时/过期上下文窗格与结构化工具卡；
- 可重复、带 SHA-256 文件所有权清单的开发安装。

Phase 2 支持项目/嵌套图层读取、字段与有界查询、选择摘要、属性/位置选择、清除选择、激活已有视图、缩放和闪烁。分析、符号化、导出、项目保存和源数据编辑仍未实现；应用不会注册任意 SQL、脚本、Shell、未知地理处理、通用 dispatcher 或系统命令工具。

## 开发入口

环境要求为 Windows、ArcGIS Pro 3.7、.NET 8 与 10 SDK、Node.js/npm、Rust/Cargo，以及已安装并可执行的 `codex.cmd`。本机 ArcGIS Pro 安装在非默认路径时显式传入：

```powershell
powershell -ExecutionPolicy Bypass -File scripts\Test-Foundation.ps1 -ArcGISProInstallDir D:\arcgis_pro
powershell -ExecutionPolicy Bypass -File scripts\Install-Dev.ps1 -ArcGISProInstallDir D:\arcgis_pro
```

预览版的安装、ChatGPT 登录、外部 Codex 检测、ArcGIS Pro 连接、重连与卸载，请参阅[预览版使用指南](docs/development/preview-user-guide.md)；控制者应只在实机观察后填写仍为 `PENDING` 的[预览版冒烟记录](docs/development/preview-smoke.md)。完整的 Visual Studio 入口、验证、事务安装、17 工具说明、启动、限制和恢复流程仍可参阅 [Phase 2 开发与使用手册](docs/development/phase-2-user-guide.md)，基础安装安全背景仍保留在[基础开发指南](docs/development/foundation.md)。DeepSeek API、提供商切换和相关配置均已明确延期，未包含在此预览版中。

## 设计依据

- [已批准产品规格](docs/superpowers/specs/2026-07-19-arcgis-pro-agent-design.md)
- [基础与可靠连接实施计划](docs/superpowers/plans/2026-07-19-foundation-and-reliable-connection.md)

## 许可证

本项目整体采用 [MIT License](LICENSE)。原始 `nicogis/MCP-Server-ArcGIS-Pro-AddIn` 样例的来源、版本与许可证保留在 [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) 和 `licenses/` 目录中。
