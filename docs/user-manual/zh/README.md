# CC Switch 二开版中文手册

本手册只说明当前二开版本的实际行为。软件为本地桌面应用，provider、路由、故障切换和会话数据均由本机管理。

## 快速入口

- [软件介绍](./1-getting-started/1.1-introduction.md)
- [安装指南](./1-getting-started/1.2-installation.md)
- [添加供应商](./2-providers/2.1-add.md)
- [切换供应商](./2-providers/2.2-switch.md)
- [路由服务](./4-proxy/4.1-service.md)
- [故障切换](./4-proxy/4.3-failover.md)
- [常见问题](./5-faq/5.2-questions.md)

## 说明

- 官方 OpenAI 登录源和第三方中转源统一使用 `custom` provider。
- 官方源和非官方源可以共同加入路由与故障切换队列。
- Codex 会话统一按 `custom` provider 展示，避免切换来源后出现两套会话历史。
- 更新按钮和自动更新流程已关闭，软件版本从本仓库 Releases 手动获取。

完整章节见上级目录中的[中文手册索引](../README.md)。
