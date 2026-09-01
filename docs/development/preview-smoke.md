# ArcGIS Pro 智能助手预览版实机冒烟记录

状态：**PENDING — 2026-08-28。此记录只能由执行 Windows 安装、ChatGPT 登录和 ArcGIS Pro GUI 操作的控制者填写；自动化验证、源码检查和历史结果不能替代实机证据。**

产品版本：`ArcGIS Pro 智能助手 0.2.0-preview.1`；目标 ArcGIS Pro：`3.7`；已验证 Codex CLI：`codex-cli 0.149.0`。

填写规则：每项只能填写 `PASS` 或 `FAIL`、日期和产品版本。不得记录邮箱、令牌、项目路径、项目名称、图层名称或工具 payload；不得粘贴登录页面、GIS 数据或原始工具内容。

| # | 验收项目 | 结果（PASS/FAIL） | 日期 | 产品版本 |
| --- | --- | --- | --- | --- |
| 1 | 安装器提供“立即启动”，且桌面与开始菜单快捷方式均可用 | PENDING | 2026-08-28 | `0.2.0-preview.1` |
| 2 | 在隔离环境或受控 PATH fixture 中验证缺少 Codex 的引导；不卸载用户的 Codex | PENDING | 2026-08-28 | `0.2.0-preview.1` |
| 3 | 找到 `codex-cli 0.149.0`；受控的不匹配兼容版本显示警告 | PENDING | 2026-08-28 | `0.2.0-preview.1` |
| 4 | 应用私有 `CODEX_HOME` 初始为未登录，随后完成官方 ChatGPT 浏览器登录 | PENDING | 2026-08-28 | `0.2.0-preview.1` |
| 5 | 自动找到 ArcGIS Pro 3.7；手动选择拒绝错误可执行文件 | PENDING | 2026-08-28 | `0.2.0-preview.1` |
| 6 | 固定 Add-In 通过 Esri 处理程序打开，重启提示正确 | PENDING | 2026-08-28 | `0.2.0-preview.1` |
| 7 | “启动 ArcGIS Pro 并连接”到达实时 Bridge/MCP 状态 | PENDING | 2026-08-28 | `0.2.0-preview.1` |
| 8 | 工具清单恰好为 17 项 | PENDING | 2026-08-28 | `0.2.0-preview.1` |
| 9 | 健康检查、项目/上下文读取和图层列出成功，且未保存或编辑 GIS 数据 | PENDING | 2026-08-28 | `0.2.0-preview.1` |
| 10 | 重启 ArcGIS Pro 后可从“正在重新连接”恢复 | PENDING | 2026-08-28 | `0.2.0-preview.1` |
| 11 | Windows 卸载移除本应用及其快捷方式，但保留外部 Codex、ArcGIS Pro、`.aprx`、地理数据库和无关 Add-In | PENDING | 2026-08-28 | `0.2.0-preview.1` |

控制者完成实际观察后，逐项把 `PENDING` 替换为 `PASS` 或 `FAIL`；FAIL 只补充不含敏感信息的最小现象描述和产品版本。
