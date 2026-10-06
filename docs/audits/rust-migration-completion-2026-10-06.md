# 七类 Rust 迁移整合验收 — 2026-10-06

基线：`f20ae414d`。七类代码迁移均已接生产、逐类提交；宿主与构建验收完成，设备验收未完成。本记录不宣称全仓库无诊断、全部历史 coverage 重跑或性能提升。

## 分类提交与边界

| 类别 | 主线提交 | Rust owner | Kotlin 保留 |
| --- | --- | --- | --- |
| 恢复归档批处理 | `5ce66f136` | `eh_archive::recovery_batch` | FD、批次取消/期限、侧车读取、Room |
| 目录观察/提升资格 | `9f5901e95` | `eh_domain::recovery_directory` | SAF 查询、权限、路径解析、最终事务检查 |
| 备份下载/标签导入 | `935ba1476` | `eh_domain::backup_import` | 备份 DB、对象复制、事务、列表/worker 同步 |
| 下载元数据补全 | `9c88926eb` | `eh_domain::download_metadata` | 文件读取、对象映射、字段写入授权、持久化 |
| 预览分页/token delta | `e73eb8ad5` | `eh_domain::spider_info::preview_tokens` | HTTP、锁、协程、token 对象应用 |
| 更新协议/选择策略 | `98d3a5171` | `eh_domain::update_policy`、`eh_net::updater` | 认证、HTTP、时钟/设置、APK 保存/安装 |
| 收藏弹窗/种子/tracker/wiki URL | `ae6d68ebf` | `eh_net::gallery::request` | 设置、浏览器/Intent |

归档批处理先在主线实现；其余六类从固定基线并行 worktree 实现，父线程逐类审查、合入、验证、提交。共享 CI、JNI 清单和 DownloadManager 在主线合并；旧元数据整计划双算候选未合入，使用删除双算后的修正版。

- 更新器空集合、标签追加位置先修复并冻结语义，再迁移。没有将字符串版本不等改为 SemVer，没有重排已有标签。
- 元数据 Kotlin 保留来源/结构/写入授权校验，不重算完整 Rust 计划。默认更新选择单次批量 JNI；自定义 callback 保留惰性调用、UTF-16 排序、异常/取消次序，不 eager 求值。
- wiki 字符串按裸 UTF-16 code units 往返，保留孤立代理项；公共 Kotlin helper/JNI 五组冻结反例均逐单位相等。不能用 UTF-8 replacement 掩盖差异。
- 不增加生产 legacy parity、失败 fallback、运行时 backend 开关。JNI 长度/数量、封闭枚举、envelope/索引、来源授权校验保留。

## 主线执行证据

日志：`/tmp/ehviewer-rust-parallel/main-integration/`（逐类）、`/tmp/ehviewer-rust-parallel/final-gates/`（整合）。日志不入仓库。

| 项目 | 结果 |
| --- | --- |
| Rust workspace/all features、integration、doctests | 575 项通过 |
| default/marshmallow JVM 全套（最终格式后） | 各 414，0 failures/errors/skipped |
| 两变体 AndroidTest Kotlin 源码 | 编译通过；不是设备执行 |
| 更新器真实 host JNI | 27 个精确 envelope；输入不变、预算、原子输出及拒绝路径通过 |
| 导航真实 host JNI | 12 个精确 response；6 个 Java 裸 wiki oracle；另有 5 个公共 Kotlin helper/JNI oracle |
| owner coverage | 各类独立 gate 通过；update_policy 127/127 行、11/11 函数，updater 161/163 行、15/15 函数，gallery request 264/264 行、22/22 函数；保持 95% 门槛 |
| Host/ARM64 Android Clippy、rustdoc | 全 features、`-D warnings` 通过 |
| 两变体 debug APK | `-PtargetAbis=arm64-v8a,x86_64`，四个实际 ABI split 构建通过 |
| Android Lint | 0 errors；default 53 warnings，marshmallow 45 warnings |
| 全部 Android JNI 导出与打包 | 四组 flavor/ABI 逐项匹配完整清单；default 113、marshmallow 112 个业务导出，另有 JNI_OnLoad；所有 retired symbols 缺席；APK 内 native bytes 与已检查 stripped 库一致 |
| Python | 12 项通过，含 CI native 路径与排序 locale 回归 |
| 格式、CI shell/YAML | Rustfmt、48 个真实迁移 Kotlin 目标 Spotless、actionlint/shellcheck、git diff --check 通过 |

宿主 JNI 首轮曾使用旧 `target/jni-probe` 库；保留失败日志，重建当前静态库并链接 host 库后重跑。未用假后端或放宽测试。APK 首轮误用了 injected ABI 参数，仅构建 ARM64；保留首轮日志，改用项目 `targetAbis` 后实际构建、核对四组产物。

最终整合另提交：`8298b7dbd` 只整理四个归档 Kotlin 文件，非注释 lexical tokens 除可选尾逗号外相同；`b80a3e80d` 修复 CI unstripped 路径为现用 AGP `merged_native_libs`，显式 C locale 排序/comm，并增加两项回归。完整 CI native shell 原文在实际 x86_64 产物上通过，未跳过 retired-symbol 检查。

`MainActivity.kt`、`KtorExtensionsTest.kt` 沿用既有冻结范围，SHA-256 前后相同；不纳入上述 48 文件格式结论。50 目标首轮失败日志保留，不把限域通过描述为全项目 Spotless 通过。

## Pi-Lens 与剩余验收

- 最终主动检查 88 个变更源码路径：53 confirmed clean、12 findings、23 inconclusive；48 项辅助提示，无已报告 error。23 中 2 项超时、21 项 push-only/silent-on-clean 无法确认。不是 88 文件 clean，更不是全仓库复扫。
- 最终格式/CI 后另检查 6 路径：3 confirmed clean、2 findings、1 超时。CI 无 unpinned-uses error，但仍有 credential persistence/excessive permissions 辅助 warning；可变校验状态、测试 stdout 和拼写提示未按业务错误机械改写。
- CI 长行整理由 `9c88926eb` 收口：42 个 coverage 正则逐字保留，28 个 Action 引用固定完整 SHA，没有升级版本或降低 owner 门槛。此轮未重跑全部历史 P1–P4 coverage gates。
- `adb devices` 无设备。Android 真机/模拟器 JNI、SAF/FD、源变化、旧 handle、协程取消、Room/通知/HTTP/安装整链及 ARM64 时延/峰值内存仍待执行。设备源码编译、host JNI、APK 构建不替代这些证据。
- 单条归档 I/O/解码无法中途取消；25 ms 是条目间协作时间片，不是硬时限。
- 更新 ZIP/APK 条目选择、JVM Regex 引擎替换、平台图像解码等属于原复审有条件/另开切片，不借本轮七类迁移宣称完成。Room、SAF、Compose、网络执行及资源生命周期仍由 Android owner 承担。
