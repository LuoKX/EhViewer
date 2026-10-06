# 风险修复与 Rust 归档检查批处理 — 2026-10-06

基线：`f20ae414d`。承接同日迁移复审；本轮先修复两处风险，再落地首个高优先级切片。记录截至本切片提交前：其余六类候选未迁移；后续按类别独立验证与提交，具体提交见 Git 历史。

开始时已有 `MainActivity.kt`、`KtorExtensionsTest.kt` 的 Pi-Lens 格式改动，以及复审报告。两个已有源码文件的字节内容经 SHA-256 前后对照保持不变；格式化仅覆盖本轮修改的 Kotlin 文件。

## 1. 先修复，不在迁移中静默改变行为

### 更新器空响应

- `GithubWorkflowRuns.latestRun()`：空 workflow runs 抛明确 `IOException`，替换 `workflowRuns[0]`。
- `GithubRelease.getDownloadLink()`、`GithubArtifacts.getDownloadLink()`：空 assets/artifacts 抛明确 `IOException`，替换无保护的 `[0]`。
- 非空行为不变：release 按服务端顺序选择首个匹配，未匹配取首项；artifacts 先按名称排序，再选择匹配或排序后的首项。
- 现有 UI 错误处理接收异常；没有把错误响应解释成“当前已是最新”。没有改变字符串版本/SHA 比较或增加线上 HTTP 测试。
- 回归：`app/src/test/kotlin/tachiyomi/data/release/GithubUpdateSelectionTest.kt`，4 项。

### 标签导入位置

- `DownloadManager.addDownloadLabel()` 使用 `planDownloadLabelImport()`。
- 新位置从当前最大 position 之后连续追加，而不是从导入数量起算；已有名称、批内重复名称均跳过。
- 保留已有行位置及导入相对顺序；新增行复制并清空旧 ID，不修改输入备份对象。
- 一次预检全部 position 的 Int 范围，溢出时不先写入部分行。不自动重排历史行，也不改变 Room 事务实现。
- 回归：`app/src/test/kotlin/com/hippo/ehviewer/download/DownloadLabelImportTest.kt`，4 项。
- 该规则仍为 Kotlin。已冻结修复后的语义，供后续下载/标签导入切片迁移。

以上 8 项先在 default/marshmallow 两个变体独立通过，再开始归档迁移。

## 2. 首个切片：恢复归档 V3 批处理

### 责任边界

- 纯 Rust owner：`eh_archive::recovery_batch`，负责版本/区间验证、累计 readable、complete、加密/数量限制/不安全路径 issues。
- FFI codec adapter：复用现有归档 reader、自然顺序和 ZIP/RAR4/RAR5/7z/tar/gzip/XZ 解码；在已有 handle 锁内检验游标及累计状态，逐条验证完整性，维护下次允许的游标。关闭行为及 FD 位置恢复不变。
- Kotlin：打开 SAF/文件 FD、批次调度、取消/总期限、读取侧车、结果映射和 Room。生产 `archiveRequest` 直接调用 V3，没有逐项 legacy 双算、fallback 或 backend selector。
- 原 V2 JNI 导出保持不变；新增 `nativeInspectArchiveRecoveryV3`，精确清单为 `crates/ffi/archive-v3-symbols.txt`。

本切片只遍历现有 reader 的可播放图像索引，不是重新实现解压器，也不是接入 `inspect_recovery_entries` 对全部归档 header 的检查。非图像条目策略、目录观察、元数据选择与恢复提升资格未借此宣称完成。

### Wire 与预算

- Request：`version = 3`、`startIndex`、`maxEntries`、`maximumEntries`、前次 Rust 返回的 `allReadable`。
- Response：`ApiEnvelope(version = 3, module = "archiveRecovery")`；payload 含索引总数、下次游标、累计状态、complete、封闭 issue 集合及当前批次条目事实。
- JNI 请求最多 256 bytes；单批最多 32 图像；单名称最多 65,536 UTF-8 bytes；响应最多 3 MiB。名称上限是新增 V3 保护，超限时失败且不回退 V2。原图像 512 MiB 上限保持不变。
- Native 每个条目后检查 25 ms 协作时间片，可提前返回非空短批次。**这不是硬性时限：单次 I/O/解码仍无法中途取消，和旧的逐项验证一样。** Kotlin 在每批前、结束后检查取消和 Android 单调时钟总期限。
- Kotlin 在应用事实前验证整个 envelope、数量、范围、游标推进、条目大小、累计状态、完成状态和 issue 形状；跨批次验证索引总数不变。类型化问题走 payload issues；协议/生命周期失败经既有 JNI 异常边界传播。错误不附文件名或凭据。
- 请求及 payload 精确 CBOR goldens 共用 `crates/archive/test_data/recovery_batch_{request,response}.cbor.hex`，Rust/Kotlin 双向验证。只在入口做防御验证，不保留旧算法作运行时 parity。

批次数量目标是减少 JNI 往返，不是已测得吞吐量、时延或内存收益；无真机性能结论。

## 3. 本轮执行证据

日志目录：`/tmp/ehviewer-rust-next-slice/`，不纳入仓库。

| 验证 | 本轮结果 |
| --- | --- |
| Pi-Lens 主动 LSP：本轮 19 个源码/测试路径 | 19 confirmed clean，0 error，0 未确认；不等于全仓库复扫 |
| default/marshmallow JVM 全套 | 各 329 项，0 failures/errors/skipped |
| Rust workspace、integration、doctests | 526 项通过 |
| Python 脚本测试与 rewrite naming | 7 项通过；命名检查通过 |
| 新 `recovery_batch` 独立 owner coverage | 57/57 行、3/3 函数，100%；只计生产 owner，不借测试文件/其他 owner 分母 |
| Host 与 ARM64 Android Clippy | 全 features，`-D warnings` 通过 |
| Rustfmt、rustdoc | 通过；rustdoc `-D warnings` |
| 两个 debug APK 变体，ARM64/x86_64 | 均构建通过 |
| 两个变体 Lint | 0 errors；default 53 warnings，marshmallow 45 warnings |
| 两个变体 AndroidTest Kotlin 源码 | 编译通过；新增 V3 FD/CRC、游标、加密/限制测试 |
| 精确 Android JNI 导出及 JNI_OnLoad | default 各 ABI 108；marshmallow 各 ABI 107，均与按 flavor 配置的预期一致；API 23 不导出 API 26 的 `copyBitmapToAHB` |
| Spotless、git diff --check | 本轮 Kotlin 文件 scoped Spotless 通过；diff 空白检查通过 |

Rust 测试覆盖短批次、累计失败、限制、加密、版本/游标/名称/尺寸错误与 wire；registry 测试覆盖 CRC 失败、FD 位置、旧 handle、未验证游标及已有多格式解码。Kotlin 9 项新增测试覆盖 goldens、协议/形状拒绝、无 fallback、跨批状态、期限及取消。

CI 新增独立 owner 95% 行/函数门槛、V3 精确导出检查，并更新其他切片的覆盖率排除项及其回归测试；没有降低已有门槛。未重新执行全部历史 P1–P4 coverage gate。

## 4. 未验证与后续

- `adb devices` 无连接设备。真实 JNI/SAF/取消及峰值内存/ARM64 时延未执行；AndroidTest 源码编译不能替代真机测试。
- CI YAML 的主动 Pi-Lens 检查另报 28 项既有 zizmor `unpinned-uses` 辅助 error（Action 引用未固定 SHA）。与 HEAD 对照全部 `uses` 引用未变，本轮未处理此安全基线；不能把 19 个源码路径 clean 描述为全仓库无错误。
- 此切片已接生产且宿主验证通过，设备验收仍待办，不宣称整项验收全部完成。
- 下一切片按复审顺序：恢复目录观察计划与提升资格；之后元数据补全、下载/标签合并、token 更新、更新器策略、剩余 URL。
