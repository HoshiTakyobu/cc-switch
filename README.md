# CC Switch 二开版

面向 Windows 的 AI 编程工具配置、路由和故障切换桌面应用。

本仓库是 CC Switch 的独立二次开发版本，不是上游项目的官方发行版。仓库首页只记录本版本实际维护的功能和构建方式，不保留上游赞助商、宣传页和多语言重复文档。

## 当前版本

`v3.20.3-unified.2`

## 二开功能

- Codex 的官方 OpenAI 登录源与第三方中转源统一使用 `custom` provider。
- 官方源和非官方源可以同时加入路由与故障切换队列。
- 故障切换时按队列顺序在所有可用源之间切换。
- Codex 会话统一按 `custom` provider 管理，避免官方账号与中转源会话分离。
- 更新按钮与自动更新流程已关闭，版本由本仓库 Release 手动维护。

## 下载

请从本仓库的 [Releases](https://github.com/HoshiTakyobu/cc-switch-self-use/releases) 下载 Windows 安装包：

- `Windows.msi`：标准安装包
- `Windows-Portable.zip`：便携版
- `windows-x64.exe`：Windows 可执行文件

## 本地构建

环境要求：Node.js、pnpm 10、Rust 以及 Tauri 2 构建环境。

```bash
pnpm install
pnpm tauri build
```

仅构建前端：

```bash
pnpm run build:renderer
```

## 使用手册

请查看[中文手册索引](docs/user-manual/zh/INDEX.md)。

## 数据与安全

这是本地桌面应用，配置、账号和会话数据保存在用户自己的设备上。升级或切换版本前，请先备份 Codex、Claude Code 等工具的配置文件。

不要把 API Key、OAuth 凭据或本地数据库提交到 Git 仓库或公开 Issue。

## 许可证

本项目沿用上游代码的 MIT 许可证，详见 [LICENSE](LICENSE)。
