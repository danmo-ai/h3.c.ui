# H3.UI

Language: [English](README.md) | **中文**

[![Release](https://img.shields.io/github/v/release/danmo-ai/h3.c.ui?label=release)](https://github.com/danmo-ai/h3.c.ui/releases/latest)
[![CI](https://img.shields.io/github/actions/workflow/status/danmo-ai/h3.c.ui/ci.yml?branch=main&label=CI)](https://github.com/danmo-ai/h3.c.ui/actions/workflows/ci.yml)
[![License](https://img.shields.io/github/license/danmo-ai/h3.c.ui)](LICENSE)

**在 Apple Silicon 上原生跑 MiniMax-H3** — Metal 优先的 `libh3` 引擎、macOS 桌面端，以及双语创作工作室。文生视频/音频、首末帧、有序 Ref2VA 参考，全程本地完成。

![H3.UI 创作页](docs/screenshots/create.png)

产品名：**H3.UI**。引擎 CLI 仍为 `h3`；控制面目录为 `~/.h3-ui/`。

| | |
|---|---|
| **安装** | [GitHub Releases](https://github.com/danmo-ai/h3.c.ui/releases/latest) · `H3.UI_*_aarch64.dmg` |
| **引擎详解** | [ENGINE.md](ENGINE.md)（CLI、Metal、预设） |
| **UI / API 结构** | [UI.md](UI.md) |
| **设计系统** | [dq-ui](https://github.com/danmo-ai/dq-ui) |

界面模式对齐 [Danmo Make](https://github.com/danmo-ai/danmo-make)：侧栏图标 · 卡片画廊 · 创作抽屉 · 中英双语。

---

## 为什么是 H3.UI

- **端侧 Metal** — C / Objective-C `libh3` + Metal / MPS / Accelerate，生成不排队云端 GPU。
- **完整产品壳** — Tauri 2 + Vue 3 + `@danqing/dq-ui`：创作 · 下载 · 设置，不是裸 CLI 包装。
- **FL2VA / Ref2VA** — 纯文生、首末帧上传，或有序图像 / 视频 / 音频参考。
- **一键拉模型** — 魔搭 · HF 镜像 · Hugging Face；HTTP Range 断点续传 + SHA-256 校验。
- **内置 FFmpeg** — 静态 `ffmpeg` / `ffprobe` 打进包；PATH 仅作兜底。
- **去噪实时预览** — 任务进行中通过 SSE 看中间帧演化。

---

## 功能一览

| 模块 | 能力 |
|------|------|
| **创作** | 提示词、分辨率、帧数/秒数、步数、种子、速度与兼容开关 |
| **首末帧** | 上传首帧（FL2VA 必填）与可选末帧 — 真文件上传，不是手填路径 |
| **Ref2VA** | 有序参考：图像 / 视频 / 音频 / 视频+音频 |
| **模型** | 下载 MiniMax-H3 到 `~/.h3-ui/models/`；显示 FL2VA / Ref2VA / Turbo LoRA 就绪状态 |
| **作品** | 本地输出卡片；打开 / 删除 |
| **引擎 CLI** | 同仓库 `./h3` 交互会话与 profile — 见 [ENGINE.md](ENGINE.md) |

---

## 环境要求

| | |
|--|--|
| **平台** | macOS **Apple Silicon**（主目标） |
| **内存** | 完整 MiniMax-H3 建议 32 GB+ |
| **磁盘** | FL2VA 数十 GB（Ref2VA 另计） |
| **源码构建** | Xcode CLT、Rust、Node.js 20+、旁路 [dq-ui](https://github.com/danmo-ai/dq-ui) |

暂未提供 Windows / Linux 桌面包。Metal 引擎面向 Apple Silicon。

---

## 从 Release 安装（推荐）

1. 从 [Releases](https://github.com/danmo-ai/h3.c.ui/releases/latest) 下载 **`H3.UI_*_aarch64.dmg`**。
2. 打开 DMG，将 **H3.UI** 拖入「应用程序」。
3. 首次启动若被拦截（未公证 / 隔离属性），请 **右键 → 打开**，或：

```bash
xattr -dr com.apple.quarantine "/Applications/H3.UI.app"
```

4. 应用内：**下载** → 选择源（国内推荐魔搭）→ 等到 FL2VA（需要参考时再等 Ref2VA）就绪 → **创作**。

---

## 源码快速开始

面向开发者。普通用户请用上面的 Release 安装。

```bash
# 旁路设计系统（前端依赖）
git clone https://github.com/danmo-ai/dq-ui.git
cd dq-ui && npm install && npm run build && cd ..

git clone https://github.com/danmo-ai/h3.c.ui.git
cd h3.c.ui

make ui      # 桌面端（拉取 FFmpeg、构建 API + Tauri）
# make web  # 浏览器 :5810 + API :7810
# make pack # 打包 .app / .dmg
```

| 产品 | 后端 | 前端 |
|------|------|------|
| **H3.UI（本仓库）** | 7810 | 5810 |
| [Danmo Make](https://github.com/danmo-ai/danmo-make) | 7800 | 5800 |
| [Danmo Work](https://github.com/danmo-ai/danmo-work) | 7801 | 5801 |

仅引擎 CLI：

```bash
make -j8
./h3 --info -d ./MiniMax-H3
```

CLI 教程与 Metal 说明见 [ENGINE.md](ENGINE.md)。

---

## 数据目录

| 路径 | 用途 |
|------|------|
| `~/.h3-ui/` | 控制面：`config.json`、下载任务、上传缓存 |
| `~/.h3-ui/models/MiniMax-H3/` | 权重（FL2VA / Ref2VA） |
| `~/.h3-ui/outputs/` | 生成的 `.mp4` |
| `~/.h3-ui/uploads/` | 首末帧与参考媒体 |

---

## 架构

```
H3.UI (Tauri)  ──►  Vue SPA (@danqing/dq-ui)
                         │  /api
                         ▼
                   h3-api (Rust / axum + SSE)
                         │  FFI
                         ▼
                   libh3.dylib  (Metal MiniMax-H3)
                         │
              内置 ffmpeg / ffprobe
```

---

## 本地 API

基址：`http://127.0.0.1:7810`

| 区域 | 接口 |
|------|------|
| 系统 | `GET /api/system/health` · `device` · `ffmpeg` |
| 模型 | `GET /api/model/status` · `POST /api/model/download` + SSE |
| 生成 | `POST /api/generate` · 任务 SSE `/api/tasks/{id}/stream` |
| 上传 | `POST /api/uploads` · `GET /api/uploads/{name}/file` |
| 作品 | `GET/DELETE /api/outputs…` |
| 设置 | `GET/PUT /api/settings` |

下载源：`modelscope` · `hf_mirror` · `huggingface`。

---

## 发布 / CI

打标签 `v*` → [release.yml](.github/workflows/release.yml) 在 Apple Silicon runner 上 `make pack` 并发布 `.dmg`。

`main` / PR 走 [ci.yml](.github/workflows/ci.yml)。

---

## 相关项目

- [Danmo Make](https://github.com/danmo-ai/danmo-make) — 本地图像 / 视频 / 音频工作室（MLX）
- [Danmo Work](https://github.com/danmo-ai/danmo-work) — 开源 AI 工作代理
- [dq-ui](https://github.com/danmo-ai/dq-ui) — 共享 Vue 设计系统
- [MiniMax-H3](https://huggingface.co/MiniMaxAI/MiniMax-H3) — 模型权重

---

## 许可证

[MIT](LICENSE)
