# ArcGIS Pro 手工冒烟验收（PENDING）

状态：**PENDING — 尚未在本次自动化任务中操作 ArcGIS Pro GUI，不得视为通过。**

执行时只使用空白或可丢弃项目，并记录脱敏结果：

- [ ] 运行 `Install-Dev.ps1`，清单只包含应用/Add-In 文件。
- [ ] ArcGIS Pro 3.7 加载 Add-In，状态按钮显示协议 `1.0`。
- [ ] 启动开发版桌面程序。
- [ ] 如已退出登录，通过官方 ChatGPT 浏览器登录完成认证。
- [ ] 右栏显示 Codex、MCP、Add-In、ArcGIS Pro、项目和活动地图。
- [ ] 发送“检查 ArcGIS Pro 连接状态”，出现一个已完成的 `arcgis_connection_status` MCP 事件卡。
- [ ] 关闭 ArcGIS Pro，15 秒内状态变为断开/过期，桌面程序不崩溃。
- [ ] 重新打开 ArcGIS Pro，桌面程序无需重启即可恢复。

完成后另存带实际日期的脱敏文本记录。不得保存令牌、完整账号邮箱或私有 GIS 路径。
