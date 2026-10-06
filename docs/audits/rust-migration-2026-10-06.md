# Rust 迁移边界复审 — 2026-10-06

基线：`f20ae414d`（`fix(core): address Pi-Lens source audit findings`）。开始时工作区干净。

> 同日实施更新：两处风险已修复；首个归档 V3 批处理切片已接生产并通过宿主验证，真机验收待办。详见 [实施记录](rust-migration-implementation-2026-10-06.md)。下文保留实施前复审快照，源码行号及“本轮未改源码”等表述属于复审阶段。

## 范围与证据限制

本轮是全仓库结构盘点与迁移边界复审，不是重新执行上一轮 806 文件的全量诊断和构建。盘点覆盖 app、core/common、core/data、core/ui、Rust workspace；对仍含规则的 Kotlin 调用链进行当前源码阅读，对照已有 Rust owner、P1–P4 迁移记录及 ADR 0007。

当前跟踪实现文件共 799 个：532 Kotlin、249 Rust、6 Java、1 C、11 Python；另有 10 个 Gradle Kotlin DSL 文件。数量包含测试和 vendor，不能用作 Rust 迁移完成率。

- 使用 CodeGraph 定位结构和调用者，Pi-Lens 模块轮廓、符号读取、AST 搜索核实当前磁盘实现。
- Pi-Lens 引用图仍提示旧提交 `86d762e0`；引用图只作导航，不作为删除或迁移的独立依据。
- 对恢复协调器、更新器、EhUrl、EhDB、Rust recovery 执行了 5 文件主动 Pi-Lens 诊断：3 文件有辅助提示，2 文件检查未确认，0 文件确认 clean。提示主要是可变状态、非必要 let 和拼写；不据此认定业务错误或全项目无错误。
- 本轮未改源码，未重跑全量构建、测试或 Android 真机验收。上一轮测试结果不当作本轮执行证据。

## 结论

主要 HTML/JSON 解析、请求计划、下载队列/进度/排序、阅读调度/抓取状态机、ComicInfo 编解码及写入策略、恢复候选判定、BFS/去重、文件名/文本区间规则、压缩包读取和 WebP 已有 Rust 生产实现。下一轮不应按 Kotlin 文件大小整类搬迁。

可继续拆出以下 **7 类规则**。优先级表达建议顺序，不代表已有性能提升证据。

| 优先级 | 候选 | Rust 落点 | 保留在 Kotlin |
| --- | --- | --- | --- |
| 高 | 恢复归档检查批处理与结果汇总 | `eh_archive` + `eh_domain::recovery`；JNI 仅适配 | SAF 打开 FD、时钟、协程取消、Room |
| 高 | 恢复目录观察计划与提升资格 | `eh_domain::recovery` / `storage_rules` | 查询目录、读取侧车文件、权限判断、路径解析/写入 |
| 高 | 本地下载元数据补全策略 | `eh_domain::comic_info_policy` 或专属下载元数据模块 | 读文件、修改 Android 对象、Room 更新 |
| 高 | 备份导入中的下载/标签合并规则 | `eh_domain::library` / `download` | 备份 DB 打开、迁移、事务、列表/worker 同步 |
| 中 | 预览分页与 page-token 更新计划 | `eh_domain::spider_info`，请求形状归 `eh_net::reader` | HTTP、锁、延迟、token 对象应用 |
| 中 | 更新器响应与选择/检查策略 | `eh_net` 更新协议 + `eh_domain` 更新策略 | GitHub 认证、HTTP、设置持久化、APK 保存/安装 |
| 中 | 剩余弹窗、种子、tracker、wiki URL 规则 | `eh_net::gallery::request` 或专属 URL 模块 | 设置读取、Intent/浏览器启动 |

### 1. 恢复归档检查：批处理，而不是重写解压器

源码：

- `app/src/main/kotlin/com/hippo/ehviewer/download/recovery/DownloadRecoveryCoordinator.kt:356–447`，`archiveRequest`。
- `app/src/main/kotlin/com/hippo/ehviewer/bridge/archive/ArchiveReaderBridge.kt:89–111`，`name/size/verify/recoveryMetadata`。

当前逐条调用 `reader.name(index)`、`reader.size(index)`、`reader.verify(index)`，Rust 已持有归档数据，Kotlin 再构造全部条目并计算 `complete`、归并 issues，之后发送回 Rust 候选检查。

建议新增有界、版本化的批次检查接口：Rust 返回条目事实、验证结果和结构化错误，核心汇总归 Rust；Android 在批次之间检查取消和期限。不要以一个不可取消的长 JNI 调用替换整个循环。原 V2 ABI 保持稳定，新增接口同步修改精确导出清单和合约测试。

收益目标：收敛数据往返与结果规则。解压/CRC 本身已有 Rust owner，不能重复实现，也不能未经测量宣称运行更快。

### 2. 恢复目录观察与提升资格：迁规则，不迁 SAF

源码：`app/src/main/kotlin/com/hippo/ehviewer/download/recovery/DownloadRecoveryCoordinator.kt:325–354,511–523`。

残留规则：

- 按 `sourceId` 排序后，挑选第一个 `.ehviewer` / `ComicInfo.xml`。
- 聚合总大小和最大 mtime，作为指纹输入。
- `recoveryPromotionPath` 判断 gid、来源格式、`gid.cbz/zip` 文件名和可提升的父目录；扫描和 EH 查询两条链共用此函数。

建议两阶段计划：先把有界目录事实发给 Rust，返回侧车源索引和聚合事实；Kotlin 读取指定文件，再调用现有候选检查。提升资格返回封闭决策/相对路径计划，Kotlin 保留 provider 路径解析和最终 Room 事务检查。

**指纹摘要和 source-key 计算已在 Rust `recovery.rs` 中；本项仅迁输入聚合，不重复迁哈希。** Kotlin/JVM 字符串排序、忽略大小写及溢出语义必须明确保留或单独批准改变。

### 3. 本地下载元数据补全：读入后的决策尚在 Kotlin

源码：`app/src/main/kotlin/com/hippo/ehviewer/download/DownloadManager.kt:653–720`，`readMetadataFromLocal`。

残留规则包括哪些下载缺 pages/tags/artists、ComicInfo 优先级、何时退回 `.ehviewer`、哪些字段需要更新。现有 Rust 已解析 ComicInfo 和 SpiderInfo，但没有替代此整段补全决策；现有 ComicInfo 写入/归档计划也不能当作已经覆盖读取补全。

建议使用两阶段批量计划：输入缺失字段快照，返回需要观察的元数据；收到解码结果后返回字段 patch 和持久化意图。Android 保留并发读取、锁、对象映射和数据库写入，不把 Room entity 跨 JNI。

### 4. 备份导入：仅迁合并规则，不迁数据库

源码：

- `app/src/main/kotlin/com/hippo/ehviewer/download/DownloadManager.kt:193–226`，`addDownload/addDownloadLabel`。
- `app/src/main/kotlin/com/hippo/ehviewer/EhDB.kt:353–396`，`importDB`。

下载导入仍在 Kotlin 做现有 gid 排除、批内 gid 去重、WAIT/DOWNLOAD 状态归 NONE；标签导入做名称冲突和 position 分配。可扩展现有 Rust library/download 计划，返回保留的源索引、规范化状态和标签位置。对象重建、ID 清空、事务和当前队列同步仍由 Android 实施。

**QuickSearch 导入、FTS 查询形状和本地收藏变更计划已迁 Rust，不再作为新增候选。** SQLite `insertOrIgnore` 等数据库约束也不应改写成内存集合后宣称迁移完成。

注意：标签导入用 `downloadLabelList.size` 而非现有标签数作 offset。现有 5 个位置 `0..4`，导入 1 个新标签会赋 position `1`，与已有位置重叠。须补回归、确认追加顺序契约；若修复，应作为独立行为变更，不在迁移中静默修正。

### 5. 预览页与 page-token 增量计划

源码：`app/src/main/kotlin/com/hippo/ehviewer/spider/SpiderQueen.kt:344–355,380–398`，`readPreviews/getPTokenFromInternet`。

previewPerPage 推导、预览页号计算、忽略空 token 和 token-map 更新仍为 Kotlin 规则。Rust `spider_info` 当前负责编解码和身份读取，不等于负责这些运行时更新。

建议返回 `previewPerPage` 与 token 更新 delta，不在每次预览响应中往返传输整个 token map。HTTP、重试执行、锁和协程生命周期继续留 Android。

先补空预览、非零预览页、position 非法/溢出、重复 position、空 token、旧响应等测试。当前非零预览页直接访问 `previews[0]`；外围会捕获失败，但是否要保留或改进此行为必须明确。

### 6. 更新器协议与更新策略

源码：

- `app/src/main/kotlin/com/hippo/ehviewer/updater/AppUpdater.kt:34–98`。
- `app/src/main/kotlin/tachiyomi/data/release/GithubRelease.kt:17–19`。
- `app/src/main/kotlin/tachiyomi/data/release/GithubWorkflow.kt:28–32`。
- `app/src/main/kotlin/com/hippo/ehviewer/util/AppConfig.kt:42–48`。

候选：检查间隔、force-check、稳定/CI 通道差异、版本/SHA 比较、artifact 排序、flavor/ABI 匹配、无匹配项处理及响应数据验证。Rust 返回候选/无更新/协议错误等明确结果，Android 执行 HTTP 和文件保存。

当前版本比较是字符串不等，不是 SemVer 排序；不能在迁移时擅自改变。GitHub 认证凭据不放进规划 DTO；挪到 native 也不等于保护凭据。

先补边界测试：空 workflow runs、空 assets/artifacts、无匹配 ABI、失效产物、相同版本/SHA。当前 `workflowRuns[0]`、`assets[0]`、`this[0]` 无空集合检查，空响应可导致越界；这是静态可确定的路径，不代表本轮做过真实 HTTP 复现。

`downloadUpdate` 还按 URL 是否以 `zip` 结尾决定处理，并直接复制 ZIP 第一个条目。可另开切片，用有界 ZIP 检查和明确 APK 条目选择替代；不要与 Comic/CBZ 图片条目策略混用。无真机性能证据，APK 解压切片优先级低于更新选择策略。

### 7. 剩余 URL 规则

源码：`app/src/main/kotlin/com/hippo/ehviewer/client/EhUrl.kt:128–150`。

`getAddFavorites/getTorrentUrl/getTrackerUrl/getTagDefinitionUrl` 仍由 Kotlin 构建。Gallery Detail/Page/MPV、列表、收藏列表和主要修改请求已有 Rust 计划；本项只覆盖剩余规则。

建议扩展现有 request owner，一次形成完整请求/导航目标，保持 Ktor 百分号编码、query 顺序、可空 key、tag 空格替换语义。URL 中的 token/key 不进入日志和错误。收藏备注响应解析已在 Rust `favorite_note`，不重复列入迁移。

## 有条件候选及不建议迁移项

- **评论正文正则**：`EhFilter` 的整体过滤决策已在 Rust，JVM Regex 内容匹配是保留的语义适配。仅在完整刻画 Java 正则、Unicode、异常和复杂度边界后考虑迁移；不能直接换 regex engine 当作等价。
- **其余 JPEG/PNG/GIF/AVIF 解码**：可单独评估软件解码，但当前走 Coil/Android 平台。没有 ARM64 解码时延、峰值内存、色彩/EXIF/动画兼容数据，不建议为了 Rust 比例替换平台路径。
- **小型展示 helper**：`EhUtils.getSuitableTitle`、`ReadableTime` 的阈值拆分等技术上可提取，但不建议新增逐项 JNI。可在已有批量核心输出中附带派生字段，资源、语言、时区和 Compose 仍在 Android。
- **未被调用的 `StringUtils.unescapeXml`**：CodeGraph、两种 AST 调用搜索及全 Kotlin literal 查询未发现调用；不是有效迁移目标。本轮未删除。
- **Room/DAO/数据库迁移、DataStore、Ktor/Cronet/OkHttp、CookieManager/WebView、WorkManager/通知、Compose、SAF、Bitmap/HardwareBuffer 生命周期**：平台/效果 owner 保留 Kotlin。
- **C/C++ 清理**：当前仅有行为无关的链接 anchor，无新的应用层 C/C++ 算法可迁。

`architecture.md` 描绘了 store/testkit 等扩展职责，但实际 Cargo workspace 当前是 codec_limits、archive、domain、ffi、image、net 加打包 facade；本轮候选可优先复用已有 owner，不为目录对称性创建空 crate。

## 建议验收顺序

1. 先刻画现有行为，更新器空集合及标签 position 问题单独提交修复或明确保留契约。
2. 先迁归档批处理、目录计划、元数据补全和下载/标签导入，再评估 token 更新、更新器、剩余 URL。
3. 每切片使用版本化 CBOR、类型化错误、长度/数量预算；Kotlin 独立验证返回计划后再执行效果。不要删边界验证，也不恢复 legacy 双算、fallback 或运行时 backend switch。
4. 增加 Rust owner 单测、Kotlin/Rust golden、Android adapter 回归；保持两个 Android flavor 和精确 JNI 导出清单。
5. 归档批次之间保留取消/期限检查，真机覆盖 SAF、旧 handle、取消和源变更。性能结论依赖 ARM64 时延和峰值内存测量。

JNI 边界建议对照 Android 官方《JNI tips》：减少数据 marshalling 的数量/频率，异步 UI 通知留同语言。文档位置：`https://developer.android.com/ndk/guides/jni-tips`。这是边界设计依据，不是本项目已测得性能收益的证据。
