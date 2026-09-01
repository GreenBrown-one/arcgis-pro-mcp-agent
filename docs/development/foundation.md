# 基础版本开发、安装与恢复

本文保留 ArcGIS Pro 智能助手 `0.1.0` 的基础构建、安装与恢复说明。当前可安装预览版为 `0.2.0-preview.1`：其用户流程、外部 Codex 前置条件、ChatGPT 登录、连接、卸载和实机验收记录见[预览版使用指南](preview-user-guide.md)与[预览版冒烟记录](preview-smoke.md)。预览自动验证入口是 `powershell -ExecutionPolicy Bypass -File scripts\Test-Preview.ps1`。当前 Phase 2 除可靠连接外，已经实现 10 个 R0 连接/上下文/图层/字段/计数/有界查询工具和 7 个 R1 临时选择/导航工具。完整工具语义和使用流程见 [Phase 2 开发与使用手册](phase-2-user-guide.md)，待人工观察项见 [Phase 2 冒烟清单](phase-2-smoke.md)。

## 前置条件与本机基线

已验证的本机组合：Windows、ArcGIS Pro 3.7（`D:\arcgis_pro`）、.NET SDK 10.0.301（同时可用 .NET 8）、Node.js 24.15、npm 11.12.1、Rust/Cargo 1.96、Codex CLI/App Server 0.144.5。Add-In 目标为 .NET 10，MCP 与共享库目标为 .NET 8。

PowerShell 的执行策略可能拦截 npm/codex 的 `.ps1` shim，因此本项目的 Windows 命令使用 `npm.cmd` 和 `codex.cmd`。确认工具：

```powershell
dotnet --info
node --version
npm.cmd --version
rustc --version
cargo --version
codex.cmd --version
powershell -ExecutionPolicy Bypass -File scripts\Resolve-ArcGISProInstall.ps1 -Candidate D:\arcgis_pro
```

最后一条必须输出包含 `bin\ArcGIS.Core.dll` 的实际 ArcGIS Pro 根目录。项目优先使用 `ArcGISProInstallDir`，其次使用 `ARCGIS_PRO_HOME`，最后读取 ArcGIS Pro 注册表安装位置；源码和项目文件不假设安装在 `C:`。

## 构建和非 GUI 验证

在仓库根目录运行：

```powershell
dotnet test McpServer.sln --configuration Release -p:ArcGISProInstallDir=D:\arcgis_pro
Push-Location apps\desktop
npm.cmd test
npm.cmd run build
npm.cmd run tauri -- build --debug --no-bundle
Pop-Location
cargo test --manifest-path apps\desktop\src-tauri\Cargo.toml
powershell -ExecutionPolicy Bypass -File scripts\Test-Foundation.ps1 -ArcGISProInstallDir D:\arcgis_pro
```

聚合脚本检查 .NET、前端、Rust、Tauri 构建、产品版本、精确 17 个 MCP 工具与 16 个 Add-In 能力、文档工具表对等、全部工具 `Destructive=false`、无 API-key 界面/代码路径、无默认 ArcGIS 安装路径硬编码、无旧 `Ping`/`Echo`/`pro.*`/任意 SQL 或 dispatcher、无旧样例项目和无生成物/运行时秘密被 Git 跟踪。它不会把手工 ArcGIS Pro 验收伪装成自动通过。

## 开发安装及文件所有权

默认安装：

```powershell
powershell -ExecutionPolicy Bypass -File scripts\Install-Dev.ps1 -ArcGISProInstallDir D:\arcgis_pro
```

The fixed `.arcgis-pro-agent-install.lock` file is deliberately retained after a run. Its presence is not evidence that an installer is still running: exclusivity comes from the open Windows file handle held for the complete recovery/install/cleanup lifecycle. Keeping the same path avoids an unsafe delete-and-recreate race between concurrent installers.

可用 `-InstallRoot` 和 `-AddInRoot` 指定测试目录。安装器在构建前解析 Windows 实体路径（不是仅比较路径字符串），检查源码、安装与 Add-In 根目录的包含/重叠关系和每一级已存在祖先；因此 8.3 短路径和 `SUBST` 盘符也不能绕过边界。实体路径无法解析时会关闭式失败。任何重解析点、目录联接、符号链接或 GIS 数据容器都会被拒绝。`.gdb`、`.sde`、`.geodatabase`、`.mdb`、shapefile 及其 sidecar、栅格或导出文件既不能作为输出，也不能作为输出的祖先目录。

安装器只接受 MCP（`.dll`、`.exe`、`.json`、`.pdb`）、桌面程序（`.exe`）和 Add-In（`.esriAddinX`）的版本化目标。它在整个恢复、安装和清理周期持有独占锁，先在系统临时目录暂存并计算 SHA-256，再以同目录备份、Windows 文件身份、固定事务日志和原子替换提交。重复运行前会严格校验旧清单结构以及每个已拥有文件的路径、长度、SHA-256 和操作时文件身份；文件被外部修改、目标未被清单拥有或旧清单不完整时会保留现场并失败。

固定事务日志分为 `applying` 与 `committed-cleanup-pending` 两个阶段。若安装进程在提交前异常退出，下一次运行会在锁内验证日志、根目录、路径、备份、暂存文件身份和摘要，再完成回滚；若清单已经提交，则只重试备份/暂存清理，不会回滚已提交文件。任何回滚或清理未完成时都会保留日志和尚需验证的文件，供下一次安全恢复，不会把竞态出现的未拥有文件当作本事务文件删除。

默认路径：

| 用途 | 路径 |
| --- | --- |
| 源码 | 当前 Git 仓库 |
| 应用开发安装 | `%LOCALAPPDATA%\ArcGISProAgent\dev\0.1.0` |
| Add-In 包 | `%USERPROFILE%\Documents\ArcGIS\AddIns\ArcGISPro\{1A0481EA-3F43-4C98-B4B5-A58C727CD115}\ArcGISProAgent.AddIn.esriAddinX` |
| 所有权清单 | `%LOCALAPPDATA%\ArcGISProAgent\dev\install-manifest.json` |
| 非敏感配置 | `%LOCALAPPDATA%\ArcGISProAgent\config` |
| 日志 | `%LOCALAPPDATA%\ArcGISProAgent\logs` |
| 管道启动状态 | `%LOCALAPPDATA%\ArcGISProAgent\runtime\bridge.json` |
| Codex 隔离工作目录（桌面端在会话开始前自动创建） | `%LOCALAPPDATA%\ArcGISProAgent\workspace` |

`-AddInRoot` 表示安装器拥有的精确 Add-In 目录，包直接放在该目录下，不再增加版本子目录。升级旧开发安装时，安装器只迁移严格清单拥有的旧 Add-In；卸载或清理也只能处理清单列出的包，不会删除未托管的同 GUID 包，也不会读取、修改或删除 ArcGIS Pro 的 `AssemblyCache`。本路径说明不代表实时加载已经通过 GUI 验收，实时结果以冒烟清单为准。

`bridge.json` 包含短期启动令牌，不能提交、分享或复制到配置。ChatGPT 凭据始终由 Codex 管理；桌面应用只调用 `account/login/start` 的 `chatgpt` 类型并打开官方 HTTPS 登录页。

## 启动和连接检查

1. 运行 `Install-Dev.ps1`，确认其打印源码、安装、Add-In、清单、配置、日志和运行时位置。
2. 打开 ArcGIS Pro 3.7 和空白/可丢弃项目。确认“ArcGIS Pro 智能助手桥接”Add-In 已加载，状态按钮显示协议 `1.0`。
3. 启动 `%LOCALAPPDATA%\ArcGISProAgent\dev\0.1.0\desktop\arcgis-pro-agent-desktop.exe`。
4. 若未登录，点击“使用 ChatGPT 账号登录”，只在系统浏览器的 OpenAI/ChatGPT 官方页面完成登录。
5. 右栏分别查看 Codex、MCP、Add-In、ArcGIS Pro、项目和活动地图状态；断开后最后一次项目/地图可保留，但必须明确标记为非实时/过期。

Phase 2 手工验收仍记录为 `PENDING`，详见 `phase-2-smoke.md`。旧基础连接清单 `foundation-smoke-pending.md` 仅保留历史基线。

## 停止和恢复

先关闭桌面应用，再关闭 ArcGIS Pro。若进程异常残留，只结束 `arcgis-pro-agent-desktop.exe`、该应用启动的 `codex app-server` 和 `ArcGISProAgent.Mcp.exe`，不要终止其他 Codex 或 ArcGIS 会话。

本阶段没有卸载命令。需要清理开发安装时，先备份并检查 `install-manifest.json`：顶层及每个文件条目的 `owner` 必须都是 `ArcGISProAgent`，路径必须位于清单声明的 `installRoot` 或 `addInRoot` 下。只逐个删除 `files[].path` 所列的应用/Add-In 文件，随后删除清单；不要递归删除配置、日志、运行时目录，更不要删除 GIS 项目或数据。Git 历史是源码恢复路径。

## 常见故障

- **找不到 Codex**：确认 `codex.cmd --version` 成功且 Codex 在 `PATH` 中；应用不会回退为 API Key。
- **缺少 runtime 文件**：先启动桌面应用使其创建 `%LOCALAPPDATA%\ArcGISProAgent\runtime\bridge.json`，然后重新加载 Add-In；不要手工编造令牌。
- **命名管道被拒绝**：确认桌面应用和 ArcGIS Pro 使用同一 Windows 用户运行，且没有一个以管理员身份、另一个以普通身份启动。
- **`protocol_mismatch`**：桌面、MCP 和 Add-In 必须来自同一 `0.1.0` 安装，协议均为 `1.0`；重新执行清单安装。
- **Add-In 未加载**：检查 `.esriAddinX` 位于传入的 Add-In 根目录，ArcGIS Pro 版本为 3.7，并查看 ArcGIS Pro 诊断日志。
- **MCP 启动失败**：运行安装目录中的 `ArcGISProAgent.Mcp.exe` 检查依赖是否完整，再查看 `%LOCALAPPDATA%\ArcGISProAgent\logs`；不要把日志中的本机路径或账号信息公开。
- **状态过期/断线**：关闭 ArcGIS Pro 后 15 秒内应显示断开或过期；重新打开并加载 Add-In 后桌面应用应自动恢复，无需重启。

## 旧样例迁移记录

对等性构建完成后移除了原 `McpServer/ArcGisMcpServer` 和 `AddIn/APBridgeAddIn` 项目。旧样例中的五个操作已作为 Phase 2 设计输入迁移到新的类型化 R0/R1 工具；旧 `pro.*` 名称本身没有保留或暴露：

1. `pro.getActiveMapName`
2. `pro.listLayers`
3. `pro.countFeatures`
4. `pro.zoomToLayer`
5. `pro.selectByAttribute`

对应的新工具中读取/计数属于 R0，缩放和临时选择属于 R1。未来 R2/R3 实现仍必须遵守批准规格中的预览、确认、备份/恢复和禁止任意脚本/命令原则。
