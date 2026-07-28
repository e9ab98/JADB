# JADB

跨平台桌面 GUI:APK 分析 / 反编译 / 重打包 / 签名 / JADX / LibChecker 规则分析。
(VSKiller 的 Tauri 2 重制版 + JADX + 规则系统。)

## 功能

- APK 信息分析(包名、版本、SDK、权限、组件)
- Apktool 反编译(资源 + 清单)
- Apktool 重打包(可选一键签名)
- uber-apk-signer 签名
- JADX Java 源码反编译
- LibChecker 风格 JSON 规则分析(permission / component / sdk / manifest)
- 签名管理(明文 JSON,chmod 600)
- 中 / 英 + 跟随系统主题

## 环境要求

- macOS 10.15 或更高(Windows 计划在 v0.2)
- Node.js ≥ 20
- pnpm ≥ 9
- Rust stable ≥ 1.78
- Android 工具链:apktool / uber-apk-signer / jadx / aapt2(首次启动可在 Settings → Tools 一键安装,自动下载到 `~/Library/Application Support/com.jadb.app/tools/`;签名时需要 `java` 在 PATH 中)

## 快速开始

```bash
# 安装依赖
pnpm install

# 开发模式(启动 Tauri 桌面窗口,带热重载)
pnpm tauri dev

# 前端单元测试
pnpm test

# 类型检查
pnpm lint

# Rust 单元测试
(cd src-tauri && cargo test)
```

## 打包(macOS)

```bash
pnpm tauri build
# 或使用 scripts/release.sh(跑完整 QA + 打 dmg)
```

产物:`src-tauri/target/release/bundle/dmg/JADB_0.1.0_aarch64.dmg`(Apple Silicon)和 `JADB_0.1.0_x64.dmg`(Intel)。首次启动可能需要**右键 → 打开**以绕过 Gatekeeper。

## 首次使用

1. 打开 JADB,进入 **Settings → Tools**,点击 Install 安装 apktool / uber-apk-signer / jadx / aapt2
2. **Signatures** 页:新建 keystore 或导入已有 keystore
3. **Analyze / Decompile / Repackage / Sign / JADX / Rules**:按需操作

## 警告

- 签名密码以**明文**存储在 `~/Library/Application Support/com.jadb.app/signatures.json`(文件权限 `0600`)。请妥善保管本机访问权限,不要在不受信任的设备上保存生产 keystore 密码。
- 本工具仅供合法的逆向分析 / 安全研究 / 调试使用。请勿用于破解、盗版或其他违反当地法律的活动。

## 已知限制

- 仅 macOS(`.dmg`);Windows(`.msi`)在 v0.2
- 仅英文 / 简体中文 UI
- LibChecker 规则的 `manifest` 类型当前仅支持简单的 `contains` 字符串匹配;完整 XPath 子集待后续扩展
- 自动更新(`tauri-plugin-updater`)暂未接入
- Logo 是占位字母 "J",后续替换

## 项目结构

```
src/                    # React 前端
  components/           # UI 组件(Sidebar + TaskPanel + shadcn-style primitives)
  views/                # 路由视图(Hello / Analyze / Decompile / Repackage / Sign / JADX / Rules / Signatures / Settings)
  features/             # 各功能模块的表单 / 列表组件(DecompileForm / ApkInfoCard / RuleReportList / ...)
  ipc/                  # Tauri invoke 类型化包装
  store/                # Zustand store(settings / tools / task / signatures)
  i18n/                 # i18next(en.json + zh-CN.json)
  styles/globals.css    # NiceSSH 移植的设计 token
  __tests__/            # Vitest 前端测试

src-tauri/              # Rust 后端(Tauri 2)
  src/
    lib.rs              # Tauri 入口,注册 commands
    commands/           # Tauri command handlers
    services/           # 业务服务(tool_manager / apk_analyzer / apk_decompiler / apk_repackager / apk_signer / jadx_decompiler / signature_manager / rule_manager / task_registry)
    config/             # Settings 持久化 + tools.json 工具清单
    progress.rs         # 进度 / 日志 / 完成事件发射
  tests/                # cargo test 集成测试
  capabilities/         # Tauri 权限白名单
  tauri.conf.json       # 打包配置(macOS dmg only)

docs/superpowers/
  specs/                # 设计文档
  plans/                # 12-Task 实施计划
scripts/
  release.sh            # 跑 QA + 打 dmg 的辅助脚本
  git-bootstrap.sh      # 仓库初始化辅助
```

## License

MIT
