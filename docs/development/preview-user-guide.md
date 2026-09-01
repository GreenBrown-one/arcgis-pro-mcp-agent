# ArcGIS Pro 智能助手预览版使用指南

1. 如果首屏提示未找到 Codex CLI，请先安装[官方 Codex CLI](https://learn.chatgpt.com/docs/codex/cli)，安装后回到应用点击“重新检测”。
2. 运行 `ArcGISPro智能助手-0.2.0-preview.1-x64-setup.exe`，完成 Windows 安装并选择“立即启动”。
3. 在应用内完成 ChatGPT 登录、ArcGIS Pro/Add-In 检查，然后点击“启动 ArcGIS Pro 并连接”。

## 首次连接

安装完成后，Windows 会提供桌面快捷方式和“开始”菜单中的“ArcGIS Pro 智能助手”快捷方式；两者启动的是同一预览版。每位用户都使用自己的 ChatGPT 订阅方案在官方浏览器页面登录。软件不会请求 OpenAI API key，也不随安装包捆绑 Codex CLI、Codex npm 包或 Codex 桌面应用。

已验证的 Codex CLI 版本是 `codex-cli 0.149.0`。其他格式正确的兼容版本会显示非阻断版本警告，而不是被拒绝；请优先升级到已验证版本。安装、升级或修正 PATH 后，点击“重新检测”重新检查当前用户 PATH 和 npm 全局命令目录中的外部 Codex CLI。

应用会自动查找 ArcGIS Pro 3.7。自动查找失败时，使用界面的手动选择操作并选择 `ArcGISPro.exe` 本身；不要选择快捷方式、文件夹或其他可执行文件，应用会拒绝不正确的选择。检查通过后，按界面提示通过 Esri 的文件关联打开随应用提供的固定 Add-In 包。若显示需要重启 Add-In 的提示，请关闭并重新打开 ArcGIS Pro，或按 ArcGIS Pro 的提示重新加载 Add-In，再回到应用继续。

## 连接和恢复

点击“启动 ArcGIS Pro 并连接”后，等待 Codex、Add-In、Bridge 和 MCP 状态变为可用。ArcGIS Pro 关闭、Add-In 尚未加载或桥接暂时不可用时，界面会显示“正在重新连接”；重新启动 ArcGIS Pro 并确保 Add-In 已加载后，连接应恢复。若状态未更新，先确认同一 Windows 用户正在运行应用和 ArcGIS Pro，再点击“重新检测”并重新执行连接步骤。

仅在空白或可丢弃项目中进行预览验证。不要在此流程中保存项目、编辑图层或源 GIS 数据，也不要记录或分享登录信息、令牌、项目路径、项目名称、图层名称或工具请求/响应内容。

## 卸载与 Add-In

从 Windows“已安装的应用”中卸载“ArcGIS Pro 智能助手（预览版）”。卸载只移除本应用及其快捷方式；不会移除外部 Codex、ArcGIS Pro、`.aprx` 文件、地理数据库或无关 Add-In。若需要单独查看或管理 Add-In，请使用 ArcGIS Pro 的 Esri Add-In Manager；不要手动删除 ArcGIS AddIns 目录或其他 GIS 数据。

## 开发者入口

开发者可运行仓库根目录的 [Open-Project.ps1](../../Open-Project.ps1) 打开 [McpServer.sln](../../McpServer.sln)，并使用 `powershell -ExecutionPolicy Bypass -File scripts\Test-Preview.ps1` 运行预览自动验证。构建安装包前，参阅 `scripts\Build-Preview.ps1` 的可选 `-ArcGISProInstallDir` 参数。
