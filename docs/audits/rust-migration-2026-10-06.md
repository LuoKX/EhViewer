# Rust 迁移边界复审 — 2026-10-06

> 2026-10-29 方向调整：完整 Rust 应用开发已暂停。本文件末尾记录基于 `253852b49` 的现有 Android 应用后续迁移复审，以及用户批准后的五类实施与验证；原七类实施前快照继续保留。

基线：`f20ae414d`（`fix(core): address Pi-Lens source audit findings`）。开始时工作区干净。

> 同日实施更新：两处风险已修复，七类代码迁移均已逐类提交并通过主线宿主/构建验证；设备验收仍待办。详见 [初始实施快照](rust-migration-implementation-2026-10-06.md) 与 [七类整合记录](rust-migration-completion-2026-10-06.md)。下文保留实施前复审快照，源码行号及“本轮未改源码”等表述属于复审阶段。

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

## 2026-10-29 现有应用后续迁移复审

基线：`253852b49`，分支 `refactor/rust-rewrite`，开始时工作区干净。用户暂停 `native/` 完整应用开发，当前只审查现有 Android 应用可以继续交给 Rust 的职责。原七类迁移已经接入，不重新列为待办。

本轮按 client、spider/download/recovery、updater、image/Coil、reader、core model/storage 边界盘点，并用 CodeGraph 核对候选的当前实现和相关调用链。以下是静态迁移建议；本轮只更新两份文档，未改业务源码，也未重新运行构建、测试或设备验收。

### 五个主要候选

| 优先级 | 候选 | 建议落点 | Android 继续负责 |
| --- | --- | --- | --- |
| 高 | 恢复扫描与现有下载记录的关联、未匹配记录标识 | 扩展 `eh_domain::recovery` / `recovery_directory` | Room 快照、事务内复核、SAF、取消和进度 |
| 高 | 下载文件命名、URL 校验信息提取、归档后清理选择 | 扩展 `eh_domain::local_pages` / `storage_rules`，URL 部分归 `eh_net` | 文件枚举、MIME 平台观察、流传输、锁、重命名和删除 |
| 中 | 更新包 ZIP 成员选择与有界提取 | `eh_archive::production_zip` 加更新包专属策略 | HTTP、临时文件/FD、下载进度、取消和安装 |
| 中 | 图片裁边是否执行、是否接受结果的策略 | 扩展 `eh_image`，复用现有 Bitmap JNI | Coil、Bitmap 生命周期、图像对象和 Compose 矩形 |
| 中 | 评论自动链接和内嵌图片尺寸规则 | 扩展评论/文本解析模块 | HTML 转 Spanned、Span 安装、Coil、资源和渲染 |

### 1. 恢复映射关联仍在 Kotlin

源码：`DownloadRecoveryCoordinator.kt:168–197,489–492`；现有 Rust：`crates/domain/src/recovery.rs:444–567,771–807`。

目录观察、提升资格和冲突裁决已经由 Rust 负责，但两者之间仍有一段纯关联：对每个已下载记录，在扫描结果中 `firstOrNull` 查找 READY、gid 相同且提升路径等于 dirname 的候选；找不到时计算 `SHA-256("database:<gid>:<dirname-or-empty>")`，再把结果送回 Rust 冲突模块。当前关联最坏为下载数乘候选数次比较。

可把当前下载快照、候选状态和提升路径一次送入 Rust，先形成保持原始顺序的关联索引，再复用现有冲突裁决。不要重写已迁移的扫描、元数据解析或最终 Room 事务。若修改现有协议，保留旧版本契约并增加独立的新版本操作。

必须冻结：多个候选的首项优先、未 READY 候选、空/null dirname、未完成下载、重复 gid、Java 字符串相等及 UTF-8 replacement 行为。这里的数据库占位标识是直接哈希字符串；现有 `source_key` / `digest_parts` 带分段长度前缀，不能直接替换成同一个摘要格式。索引化是可评估的算法改进，收益需结合 JNI 快照大小测量。

### 2. 下载落盘规则还有三个缺口

源码：`SpiderDen.kt:195–237,326–338,455–458`；已有 Rust：`local_pages.rs:15–31`、`storage_rules.rs:18–30`。

- `perFilename` 仍使用默认 Locale 的 `"%08d.%s".format(index + 1, extension)`。
- `saveFromHttpResponse` 仍用 `/h/([0-9a-f]{40})` 从最终响应 URL 提取预期 SHA-1，并决定是否校验；SHA-1 计算本身已有 native 实现，不重复迁移摘要算法。
- `postArchive` 仍用 `^\d{8}\.\w{3,4}` 的全串匹配选择待删除文件，并决定清理侧车和临时目录。

适合拆成下载文件计划及一次目录清理计划，返回原始观察数组的索引；Android 在现有生命周期点执行效果。命名不要为每个小 helper 单独增加 CBOR 往返，可纳入已有下载/文件批次。

兼容重点：索引加一的 Int 溢出、默认 Locale 数字、空扩展名、临时后缀、正则 Unicode 行为、URL 首个匹配及大小写。`local_pages::page_number` 使用八位 ASCII 数字和明确图片扩展名，不能直接代替现有清理正则。`postArchive` 的当前前提是归档存在；若要改成“重新验证归档后才删源文件”，应独立定义行为修复。Rust 返回删除索引后，Android 应校验其对应的文件快照并保持现有删除时机。

### 3. 更新包处理可复用 Rust ZIP 基础

源码：`AppUpdater.kt:52–79`；已有 Rust：`production_zip.rs:88–159`。

更新协议解析和版本/产物选择已迁移；下载后的容器处理仍按 `url.endsWith("zip")` 分支，ZIP 分支直接读取 `nextEntry` 并写入目标文件。它尚未明确选择 APK，也未定义空 ZIP、首项为目录、多个 APK 等情况的产品规则。

建议独立定义更新包成员策略，复用 Rust ZIP 的目录预检、成员身份、CRC 和长度检查，给更新包设定专属预算。不能套用漫画阅读器的图片过滤或自然排序。

现有 `production_zip` 需要 `Read + Seek`，当前 HTTP 输入流不支持 seek。实现时需由 Android 下载到受限临时文件并提供 FD，或另行设计有界流式接口，不能直接把当前流传给现有 reader。明确 APK 选择、内容识别和临时文件提交会改变现有行为，应先定契约；安装仍由 Android 承担。

### 4. 裁边算法已在 Rust，接受策略尚在 Kotlin

源码：`DetectBorderInterceptor.kt:25–49`、`crates/ffi/src/jvm/bitmap.rs:28–48`、`crates/image/src/border.rs`。

Rust 已负责像素裁边检测和二维码检测。Kotlin 仍决定：存在二维码时跳过、长宽整数比小于 2 才检测、裁剪结果宽高均严格大于原尺寸的 75% 才采用。

适合把“是否检测及是否接受矩形”纳入现有 Rust 图片操作，返回保持原图或裁剪矩形，避免为几个比较新增一次 JNI。Coil 请求开关、对象类型判断和 Bitmap 生命周期留在 Android。

必须保持整数除法、Float 精度、严格大于边界、二维码已知状态及异常顺序。若未来合并 QR/裁边调用，还需保持现有拦截器顺序与缓存行为，不能把两项检测无条件执行。

### 5. 评论显示前仍有确定性解析规则

源码：`GalleryCommentsScreen.kt:127–178,437–447`、`ui/legacy/CoilImageGetter.kt:16–27`。

评论 HTML 响应解析已经在 Rust；Android HTML 转换之后仍用固定正则补自动链接，跳过已有 URLSpan 的区间。内嵌图片则从 URL 提取原始宽高和目标宽度，再计算占位尺寸；不匹配时使用 200×300。

可让 Rust 接收转换后的文本及已有链接区间，批量返回新增链接的 UTF-16 区间；图片 URL 解析和尺寸计划可在同一评论预处理边界完成。HTML/Spanned 和 Coil 渲染保留当前实现，不能直接用原始 HTML 的位置替代显示文本位置。

验收重点：emoji/孤立代理项、现有链接重叠、括号和标点、空匹配、过长数字导致的转换异常、零尺寸、默认宽度、整数运算顺序、Coil FIT 缩放及舍入。固定链接语法与用户保存的任意正则是不同切片；后者仍需单独验证 Android ICU 行为。

### 较低收益的可选项

- 阅读器点击区域与按键动作：`ViewerNavigation.kt:24–49`、`KeyEventHandler.kt:15–27` 的区域翻转、首个命中、方向/音量键映射可以归入 Rust 规则；`PagerViewer.kt:199`、`WebtoonViewer.kt:63` 执行翻页。规则量较小，若迁移，应在配置变化时生成区域计划，避免每次指针移动跨 JNI。Compose 事件和动画保留。
- 下载/收藏交互前置决策：`CommonOperations.kt:121–232` 的默认标签、是否弹窗、收藏选项到 slot 的映射仍可提取；队列状态机和真正的本地收藏变更计划已经在 Rust。收益主要是集中测试，不是替换对话框。
- 收藏名称/计数快照：`Settings.kt:73–75` 与 `FavoritesViewModel.kt:68–70` 仍负责十分类偏好投影和总数更新。可扩展现有收藏响应/计划，不值得为属性读取新增逐项 JNI。暂停中的 `native/` 偏好存储及未集成收藏策略不能视为现有应用已经迁移，也不在本轮继续集成。
- Cookie 小规则、标题选择、时间显示和 Room 标签字符串转换：体量较小且靠近平台，优先与已有批次合并，暂不单独开迁移。

### 保留边界及建议实施顺序

`EhFilter` 用户正则和恢复扫描 `STRICT_PAGE` 当前刻意由 Android 提供匹配事实；不能以 Rust regex/ASCII 扩展名判断替换后宣称等价。Room/DAO、DataStore、Ktor/Cronet/OkHttp、WebView CookieManager、SAF、通知/后台服务、Compose 和 Bitmap 生命周期继续保留。其余平台图像解码另行评估设备色彩、动画和内存兼容。现存 C 文件仅是链接 anchor；`StringUtils.unescapeXml` 当前搜索仅见定义，不列为有效迁移工作。

建议先做恢复映射关联，再做下载文件规则；更新包选择先明确行为契约，裁边策略和评论解析随后。每项复用已有 Rust 模块、保留独立边界验证、增加有界版本协议与差分用例。继续沿用 owner 独立覆盖率、Kotlin/Rust wire、两个 flavor/ABI 和相关设备测试要求。以上为批准前的审查结论；后续实施记录如下。

## 2026-10-29 批准后的五类实施

用户批准继续后，五类逻辑接入现有 Android 应用。完整 Rust 应用及其收藏快照集成继续暂停，`native/` 未修改；现有 Cargo 依赖和锁文件未改变。

| 类别 | Rust 实现 | 实际调用边界 |
| --- | --- | --- |
| 恢复关联 | `eh_domain::recovery_mapping` | 扫描协调器一次发送候选/下载快照，Rust 以索引关联代替逐下载线性搜索，再进入原有冲突裁决 |
| 下载文件规则 | `eh_domain::download_files` | `perFilename`、响应 URL 哈希提取和归档后的批量清理索引直接调用 Rust |
| 更新包 | `eh_archive::update_package` | 下载先暂存为可 seek 文件，通过 FD 调用已有 ZIP 编解码器，再发布提取结果 |
| 裁边策略 | `eh_image::crop_policy` | 原生调用内先决定是否检测，再决定是否采用矩形；二维码跳过发生在尺寸/像素读取之前 |
| 评论预处理 | `eh_domain::comment_display` | 已转换的评论文本交给 Rust 识别链接；图片 URL 交给 Rust 解析源/目标尺寸 |

### 保留的语义与平台职责

- 恢复关联保持首个 READY 匹配、nullable 路径相等、原始下载顺序、未配对 UTF-16 字符及数据库占位标识的旧哈希格式。Rust 不读 Room 或打开路径。输入各最多 100,000 项、路径合计 16 MiB、编码请求最多 32 MiB；旧冲突接口及事务检查保留。
- 文件名保留 Int 加一溢出、八字符最小宽度、负号、默认地区数字和原始 UTF-16 扩展名。平台提供 `DecimalFormatSymbols.zeroDigit`。固定 URL 哈希语法只取首个匹配的 40 位小写十六进制；SHA-1 计算继续复用旧 native 实现。小操作采用 primitive JNI，不逐名称封装 CBOR。
- 清理仍由 Android ICU 对原文件名正则提供匹配事实，Rust 返回一次完整、有序的源索引；Android 校验全部索引后才删除。归档存在条件、侧车/临时目录的处理以及延迟到 reader 停止后的时机保留。未借此替换平台 Unicode 正则或增加归档重新验证行为。
- 裁边保持整数长宽比、Float 75% 严格阈值及异常顺序；原像素算法复用。Coil 和 Bitmap 生命周期保留。
- 评论以 raw UTF-16 传输。Android 提供当前平台 `\d` 分类和 `Character.digit` 事实，Rust 执行固定链接/图片 URL 语法；既有 URLSpan 的重叠处理、HTML 转换、Coil FIT 和舍入保留。图片源尺寸溢出仍抛 `NumberFormatException`，目标宽度转换失败仍用 200，目标高度仍按原 Int 运算顺序计算。
- 新预算：扩展名 4,096 code units、图片 URL 16,384 units、清理观察 100,000 项、评论/内嵌图源 131,072 units。超限显式失败，不回退 Kotlin 算法。

### 更新包的明确行为调整

ZIP 更新包现在要求恰好一个普通、非空、未加密、路径形状合法的 `.apk` 成员（扩展名忽略 ASCII 大小写）。目录和说明文件可以位于它之前；无 APK、多 APK、无效成员或损坏 CRC 均失败。选定成员不使用漫画图片过滤/自然排序，也不按成员路径写文件。

下载和提取各限 512 MiB，更新包最多 256 个目录记录；共享 ZIP 预检另有自己的元数据上限。原 URL `endsWith("zip")` 分支和 HTTP Accept 行为保留。普通 APK 下载也先写同目录临时文件，完成并同步后才发布。ZIP 提取的输入/输出 FD 必须为不同普通文件，调用者 FD 保持所有权，源偏移保持不变。失败/取消清理本次临时文件，不发布部分内容；APK 安装继续由原 Android 流程处理。

同步 JNI 解压期间的取消在返回后检查，发布前再次检查；这不是可中断单次解码的时间片。签名、安装和设备行为仍属 Android 验收范围。

### 验证记录

| 检查 | 当前结果 |
| --- | --- |
| Rust 全 workspace/all features、integration、doctests | 593 项通过 |
| Host Clippy、rustdoc | 全 workspace/all targets/all features，`-D warnings` 通过 |
| Android Clippy | ARM64、全 features、`-D warnings` 通过 |
| 恢复关联独立覆盖率 | 行 86/86，函数 12/12 |
| 下载文件独立覆盖率 | 行 59/59，函数 8/8 |
| 更新包独立覆盖率 | 行 48/49，函数 4/4 |
| 裁边策略独立覆盖率 | 行 21/21，函数 1/1 |
| 评论预处理独立覆盖率 | 行 151/151，函数 20/20 |
| 真实 host JNI | 恢复 55 行 Java oracle；文件名 168 个 Locale oracle；链接 2,384 例及图片/数字 3,382 例；真实 FD 更新提取、所有权/偏移、别名和无效包拒绝通过 |
| Python、CI shell/YAML | 42 项脚本测试、actionlint、shellcheck 通过；原 42 个隔离 coverage mask 均保留并加入新 owner |
| 两变体 JVM 全套 | default/marshmallow 各 430 项，0 failures/errors/skipped |
| 两变体 AndroidTest 源码 | Kotlin 编译通过 |
| Android APK | 两变体各 ARM64/x86_64，共四个实际 split 构建通过 |
| Android JNI 与打包 | default 两 ABI 各 122、marshmallow 两 ABI 各 121 个导出（含 JNI_OnLoad），逐项匹配清单；retired symbols 缺席；APK 内库与检查过的 stripped 库逐字节相同 |
| APK 校验 | 四组 ELF ABI、debug 签名、16 KiB ZIP 对齐检查通过；哈希记录于 `/tmp/ehviewer-followup-artifacts.json` |
| CI release lint | `lintMarshmallowRelease` 完成，0 errors、2 warnings（既有资源拼写、默认 ABI 配置提示） |
| 本轮 Kotlin 格式 | 21 个新建/修改文件，显式范围且关闭 ratchet 的 `spotlessKotlinCheck` 通过；包括未跟踪新文件 |
| 全局格式 | `spotlessCheck` 仍仅在未修改的 `MainActivity.kt`、`KtorExtensionsTest.kt` 报既有格式问题；未扩展到这两个冻结文件 |
| Android 设备执行 | `adb devices` 无设备；新增设备测试源码覆盖实际 ICU、地区数字、Bitmap 和 FD，不记录为已执行 |

五个 owner 使用各自至少 95% 行/函数 gate；没有用合并平均数替代独立门槛。裁边首轮 LLVM 泛型实例统计为 19/21，改为同一生产 monomorph 的测试驱动后完整执行原有全部场景，得到 21/21；生产算法及 gate 均未放宽。修正过测试区间计数、Ktor 失败 channel API、格式工具目标和 Clippy 文档问题，失败记录保留在本次日志中。

复现入口：`scripts/check_followup_coverage.py` 与 `scripts/test_followup_jni.sh`。后者要求先构建当前 `ehviewer_rust --features jvm`，每次重新链接当前静态库；不加载历史 probe 作为新代码证据。新增 7 个 JVM V1 和 1 个 Android image V1 导出，精确清单及 Android V1 检查同时维护。

本轮日志位于 `/tmp/ehviewer-followup-*`，以及首项恢复关联的 `/tmp/ehviewer-recovery-mapping-*`；未安装或发布 APK。

## 五类实施后的后续复审

本次基于 `253852b49` 加当前未提交的五类实现审查，使用工作区源码，不把 HEAD 当作最新实现。完整 Rust 应用继续暂停。本轮只更新迁移文档；不继续实施新功能，不重复计算前两轮已接入的 Rust owner。

剩余候选主要是入口状态转换、操作规划和事务内字段策略，规模比前两轮小。优先收口已有 Rust 规则尚未覆盖的入口，然后扩展少量可集中测试的业务规则；不按 Kotlin 文件数量制定迁移比例。

### 优先级与边界

| 优先级 | 候选 | 当前 Kotlin 职责 | 建议 Rust 落点 |
| --- | --- | --- | --- |
| 优先 | 下载启动状态归一化入口 | 查询后先把 WAIT/DOWNLOAD 改成 NONE，再进入 Rust 恢复流程 | 复用 `eh_domain::download`，厘清启动与普通查询语义 |
| 中 | 搜索条件及分页状态转换 | 页数筛选校验、cursor/range/seek 互斥、排行榜页号和前后页 | `eh_domain::search` 或列表状态模块，复用 `eh_net::gallery::list_url` 输出 URL |
| 中 | 下载标签与操作前置计划 | 标签新增/改名/删除/改分组；默认标签、收藏 slot 与弹窗选择 | 扩展下载/库规则；复用已迁入的队列及收藏变更 owner |
| 中，事务约束较强 | 恢复提升时的元数据合并 | 正式数据优先、缺失字段补齐、artist trim/去重/顺序 | `eh_domain` 的恢复合并策略，事务内同步调用 |
| 较低 | 阅读器交互规则 | 点击区域、翻转/RTL、先平移还是翻页、按键、滑块页码 | 扩展 reader 规则；事件和动画仍由 Compose/Telephoto 执行 |
| 有条件 | 下载缩略图的本地/远端选择 | 本地缩略图尝试、远端结果、缓存落盘和旧 JPEG 清理的顺序 | 与已有 `eh_net::cache_keys` 相邻的有限状态计划 |

### 1. 下载启动入口存在已有规则的重复实现

证据链：

- `EhDB.kt:76–81`：`getAllDownloadInfo()` 在 `joinList()` 后用 Kotlin 直接将 WAIT、DOWNLOAD 改为 NONE，只修改返回对象。
- `DownloadManager.kt:100,133–140`：先取得上述列表，之后才启动 `recoverRustDownloadQueue()`。
- `DownloadQueueAndroidAdapter.kt:46–62`：恢复输入只保留原状态为 WAIT/DOWNLOAD 且不在当前 worker/FIFO 中的行。
- `DownloadManager.kt:428–439`：空恢复输入直接返回；只有 Rust 产生的 changes 才在这里调用 `EhDB.putDownloadInfo()`。
- `crates/domain/src/download.rs:50–54,381–401`：相同状态归一化已经由 `after_process_restart` / `recover_download_queue` 实现。

因此，启动查询预先改成 NONE 的行不会再作为这些原始易失状态进入 Rust 恢复分支。这是应先收口的入口遗漏，不需要再写一套状态机。

`getAllDownloadInfo()` 还用于当前恢复扫描（`DownloadRecoveryCoordinator.kt:177`）。实施前要分别刻画“启动恢复”和“扫描时读取”的契约，保留原始快照并复用既有 Rust 决策；不能简单在每次数据库查询时无条件执行重启恢复或回写，否则可能涉及仍在运行的队列。是否把原本只修改内存的查询改成数据库回写，应独立明确，不能藏在语言迁移中。回归至少覆盖启动/二次读取、在跑 worker/FIFO、非活动易失状态和完成/失败状态。

### 2. 搜索与分页仍有前端状态规则

源码：`SearchFilter.kt:75–87,125–127,152–187`、`ListUrlBuilder.kt:52–87`、`FavListUrlBuilder.kt:37`、`GalleryListViewModel.kt:35–69`、`GalleryListScreen.kt:598–622`。

Rust 当前 `GalleryListUrlPlan` 只返回 URL；它已负责请求编码，尚未替代这些 UI 状态转换：

- 页数下限输入限制在 0–1000，上限限制在 0–2000；非零上下限要求 `min <= min(max/2, max-20)`，只给上限时要求 `max >= 10`。
- 分类/高级选项 bit 切换、评分菜单索引与值的转换。
- `setIndex` 清空 range/seek；`setRange` 清空两个 cursor 和 seek；`setSeek` 先设 next=`"2"` 再保存日期。
- 排行榜的 1-based 页号与内部 seek 字符串转换、200 页边界及前后页；普通列表的 prepend/append/refresh cursor 转换。
- QuickSearch 名称最后一个 `@` 后的进度恢复。

建议按一次逻辑编辑/导航操作返回完整新状态，再复用现有 URL builder，不重新迁 URL grammar。Paging、SavedStateHandle、Mutex、HTTP、资源文字和随机数来源继续由 Android 提供。保持非法数值转 0、即时输入显示、Int 溢出、null/空字符串和历史 seek 语义；不要只在提交时校验而改变输入过程。

### 3. 标签管理与操作选择不等于已迁的导入/队列

源码：`DownloadManager.kt:605–663`、`CommonOperations.kt:121–232,406–432`、`EhDB.kt:181–203`。

已有 Rust `backup_import` 只覆盖备份导入时的名称冲突、追加位置；`download` 覆盖队列；`library` 覆盖实际本地收藏动作。仍可提取：标签新增/改名/删除/改分组计划、受影响下载索引、默认标签是否仍有效、是否需要选择标签、收藏弹窗项目及所选项目到 slot 的转换。对话框、数据库、列表对象、锁和通知保留。

这里有需要先冻结的差异：普通 `addLabel` 使用列表 size 作 position，而备份导入使用最大 position 加一；`deleteLabel` 内存侧按删除位置之后的列表元素减一，DAO 的 SQL 按 `POSITION > :position` 减一；不存在的标签会走到 `get(-1)`。稀疏/重复位置、改名冲突和缺失标签必须先有表征用例，不能直接套用导入算法或顺手重排数据。

数据库 `LABEL` 唯一约束及下载记录的 `ON UPDATE CASCADE` / `ON DELETE SET NULL` 已有 SQL 语义，应保留。Rust 返回操作计划，不再另算一套数据库级联结果。

### 4. 恢复提升事务里的字段合并仍可独立迁移

源码：`DownloadRecoveryDao.kt:163–206`；调用入口 `EhDB.kt:117–134`。

`mergeGallery` 按字段保留正式元数据：非空白标题/日文标题/缩略图/时间/上传者/语言、非零分类和评分、非空 tags、正页数，并保留正式记录的 favoriteSlot。artist 列表按“旧记录 + 新记录”顺序 trim、去空、稳定去重，空合并结果不执行 artist 重写。这些都是可单测的纯策略。

它与已有 `download_metadata` 不同：后者处理下载侧车读取后的 pages/tags/artists 补全，缺失判定和覆盖规则也不同，不能直接复用为同一个合并操作。

只提取字段选择/合并计划。候选身份、token、页数、路径占用和活动下载的复核必须保持在 `@Transaction` 内，策略也要消费事务内的最新快照；不能先在事务外合并后直接写入。还需解决 `core/data` 到应用 JNI 的依赖方向，使用事务内同步策略边界，保留 Room 引擎。回归包含 Kotlin blank/trim、null 与空列表、NaN/正负零评分、favoriteSlot 保留、artist 顺序及失败回滚。

### 5. 阅读器交互可以规划化，但避免跨 JNI 执行动画

源码：`ViewerNavigation.kt:24–49`、`KeyEventHandler.kt:15–27`、`PagerViewer.kt:190–265`、`WebtoonViewer.kt:58–76`、`SliderPagerDoubleSync.kt:25–49`。

可集中点击区域生成/翻转、首个命中、RTL 的 NEXT/PREV/LEFT/RIGHT 映射、剩余可平移距离大于 1 像素时先平移、音量/方向键映射、滑块 1-based 页码和页边界。已有 Rust 阅读调度与 fetch reducer 继续复用。

Android 提供当次 viewport/content bounds、缩放和设置观察，并执行 pan/scroll；配置变化时缓存区域计划，离散点击/按键时调用一次。不要把每帧坐标更新、Compose 对象或 Telephoto 动画搬入 JNI。保留区域端点、首个匹配顺序、NaN/零尺寸和 RTL 组合，收益主要是规则集中与回归覆盖。

### 6. 缩略图选择需要保留分阶段效果顺序

源码：`DownloadThumbInterceptor.kt:31–61`。缩略图 key 生成已归 Rust；剩余流程是更新图库 thumbKey、尝试本地 `thumb.<format>`、本地解码失败后尝试远端、远端成功且目录存在时从缓存写回、随后清理旧 `thumb.jpg`。

适合有限状态计划，而非再迁一次 cache key。必须区分 ErrorResult、异常和取消，保留本地成功的提前返回、缓存 miss、目录消失、不同后缀和旧 JPEG 清理顺序。Coil 请求、文件观察、缓存读写和 Room 更新仍在 Android。规模较小且与 I/O 顺序耦合，优先级低于前述入口和业务规则。

### 有条件或低收益项

- `SessionController.kt:96–115,136–154,192–195` 的请求代次和一次性结果接收可设计成纯 reducer，但必须保留加载前/后的两次检查、规划前消耗 token、部分提交及 NonCancellable 顺序；生命周期模式决策已经在 Rust。暂不作为下一批首选，不移动 CookieManager、凭据或协程 Mutex。
- `HardwareBitmapInterceptor.kt:38` 与 `CropBorderInterceptor.kt:39–55` 的硬件尺寸阈值可归并到图像策略，但 SDK 能力、格式、Bitmap/HardwareBuffer 所有权和失败后软件拷贝仍是平台适配；不新增仅比较一次 max(width,height) 的 JNI。
- 收藏十分类快照、少量标题/时间/显示格式、Cookie 名称过滤、归档排序开关仍可提取，建议随现有批次合并。暂停中的 `native/` 收藏策略不在本轮继续整合。

建议下一批先处理启动状态入口，再做搜索/分页状态与标签/操作规划。恢复入库合并需要单独验证事务边界；阅读器和缩略图在上述工作之后。现有 Compose、Room/DataStore、网络执行、SAF、设备图像解码和系统服务保持当前职责。

本轮为静态源码复审，没有重新运行上一轮构建/测试。已对照 `/tmp/ehviewer-next-audit-source-before.json` 核对 app/core/native/scripts/.github 下 1,708 个既有文件：0 修改、0 新增；保留上一轮全部未提交实现。核对结果在 `/tmp/ehviewer-next-audit-source-check.json`，文档差异通过 `git diff --check`。


## 2026-10-29：第二批获批实施记录

用户在上述复审后批准继续。本批限定为前三组：下载启动状态入口、搜索/分页状态、标签管理与下载/收藏操作入口。新增四个 `eh_domain` owner 与五个 JNI V1 导出；继续使用原应用 `app/src/main/rust/`，`native/` 保持暂停。

- **启动投影**：`EhDB.getAllDownloadInfo` 把原始状态数组一次交给 Rust，完整验证响应后修改新查询对象。`download_startup::normalize_state` 复用 `DownloadState::after_process_restart`，备份导入也改用同一函数。未知 signed 状态、顺序和重复/非正 gid 保留。查询仍不回写数据库，运行中的 worker/FIFO 恢复逻辑保留。
- **搜索/分页**：`catalog_state` 接管类别/高级选项切换、评分索引、页数输入范围与校验、排行榜页码/边界、cursor/range/seek 转换和 QuickSearch 最后一个 `@` 后缀。页面 setter 保留其他 cursor/range；日期跳转仍设 next=`2`。可编辑数字由 JVM 解析，业务算术使用 Java Int wrapping；文本按原始 UTF-16LE 传输。已有 URL 编码、Paging/HTTP、随机数、资源文字和 SavedStateHandle 保留。
- **标签**：`label_rules` 规划首个精确名称匹配、按当前列表 size 追加、分组许可、受影响下载 slots、删除后的列表后缀 position 变化。保留 null/空串、大小写、孤立代理字符、重复名、稀疏/重复 position 及 signed 溢出。缺失删除仍在 Room 调用之前由 `get(-1)` 失败；重命名/删除数据库返回后，匹配下载的计划在原 sortMutex 内读取新快照。Room 外键级联、SQL fill、唯一约束和原有多阶段效果顺序保留。
- **操作入口**：`entry_actions` 规划稳定 existing/new 分区、默认下载标签/弹窗选择、收藏直达/菜单/备注获取和所选项到 slot 的映射。通知权限仍最先执行，既有下载先启动，无新增时仍提前返回；弹窗后的标签存在性复核和设置保存顺序保留。收藏菜单是否有“移除”项在弹窗前捕获，选中项按该快照换算。

传输按快照和文本预算拒绝超限输入，不增加 Kotlin 业务 fallback。新增真实 JNI Java 差分、Rust/Kotlin/Java 共用 CBOR goldens、Kotlin 适配测试与设备测试源码；四个新 owner 分别以生产文件作覆盖率分母。CI 导出清单和原有 42 个覆盖率排除规则同步更新。

最终验证（当前工作树）：

| 检查 | 结果 |
| --- | --- |
| Rust workspace / all features / integration / doctests | 607 项通过，0 失败、0 忽略 |
| JVM defaultDebug / marshmallowDebug | 各 442 项通过，0 失败、0 跳过；加载本轮重建的 host JNI 库 |
| 新 owner 独立覆盖率 | startup 19/19 行、2/2 函数；catalog 87/87、10/10；label 62/62、7/7；entry 56/56、2/2，均 100% |
| 真实 JNI 差分 | 100,000 个 raw state；855 组 JVM 数值规则；256 种稳定分区、32 种默认选择；两套共享 CBOR golden 与非法边界；上一批四组 JNI 回归通过 |
| Android 测试源码 | 两变体的 AndroidTest Kotlin 编译通过，新增设备用例覆盖五个 JNI 入口；adb 无设备，未执行设备测试 |
| APK / JNI / ABI | default + marshmallow × ARM64 + x86_64 四份 APK 构建通过；精确导出各 127 / 126（含 JNI_OnLoad），退役符号阴性，包内库与 stripped 库逐字节一致，ELF ABI、签名、16KB zipalign 通过 |
| Rust 静态检查 | host workspace/all-features/all-targets Clippy `-D warnings` 与 rustdoc `-D warnings` 通过 |
| Android lint | `lintMarshmallowRelease` 0 错误、2 个既有警告：arrays.xml:169 Typos、build.gradle.kts:37 ChromeOsAbiSupport |
| 格式与 CI | 全部变更 Kotlin 的 scoped Spotless 通过；42 项脚本测试、actionlint、shellcheck、ruff、`git diff --check` 通过 |

本轮格式检查覆盖未跟踪的新 Kotlin 文件，并关闭 ratchet；未改动上一轮记录的 MainActivity/KtorExtensionsTest 全局格式遗留问题。源码快照核对：本轮 18 个既有文件变更、28 个新增文件；`native/` 下 414 个既有文件未变，上一批实现均保留。日志及证据：`/tmp/ehviewer-round2-{rust-tests,android,jni,coverage,clippy,rustdoc,lint,artifact-check}.log`、`/tmp/ehviewer-round2-artifacts.json`、`/tmp/ehviewer-round2-source-check.json`。复现仍使用 `scripts/test_followup_jni.sh` 和 `scripts/check_followup_coverage.py`，现覆盖两批 owner。

恢复提升事务合并、阅读器交互和缩略图流程留在后续；本批未提交 git、未安装 APK。


## 2026-10-29：第二批之后的剩余规则复审

基线是包含前两批未提交迁移的当前工作树（HEAD 仍为 `253852b49`）。本轮使用 CodeGraph、当前源码和调用点检索，定向复审目录/收藏、恢复入库、阅读器、缩略图及设置/会话。本轮授权是继续审查；仅更新审查记录，不新增业务实现，`native/` 保持暂停。

### 优先级与复用方向

| 顺序 | 剩余部分 | 当前 Kotlin 入口 | Rust 落点与边界 |
| --- | --- | --- | --- |
| 1 | 收藏页导航、快速搜索创建和编辑校验 | FavoritesScreen、GalleryListScreen、DownloadsScreen | 复用 `catalog_state`、`library`、`label_rules` / `entry_actions`，补齐直接修改字段的入口 |
| 2 | 标签/快速搜索的拖动和位置计划 | DownloadsScreen、GalleryListScreen | 共享位置规划；Android 保留拖动反馈、列表对象和 Room 执行 |
| 3 | 收藏批量移动/删除及本地状态覆盖 | FavoritesScreen、EhEngine.fillInfo | 扩展 `entry_actions` / `library`；继续复用 `eh_net::gallery::favorite_mutation` 的 HTTP 计划 |
| 4 | 恢复入库字段合并和 artist 归并 | DownloadRecoveryDao.promoteVerified | 独立恢复合并策略，读取和决策均在现有 Room 事务内 |
| 5 | 阅读器导航、平移/翻页和缩放选择 | ViewerNavigation、PagerViewer、WebtoonViewer、Utils | 扩展 reader 规则；按离散操作规划，动画/坐标观察留在平台 |
| 6 | 下载缩略图本地/远端流程 | DownloadThumbInterceptor | 分阶段计划；Coil、缓存、文件和数据库效果留在 Android |
| 后续合并 | 收藏分类快照、排序偏好及扫描收尾 | Settings、FavoritesViewModel、SortMode、DownloadRecoveryCoordinator | 批量或随相邻功能收口，避免为几行算术增加独立 JNI |

### 1. 已迁规则仍有未接入的 UI 入口

**收藏页导航**：`FavListUrlBuilder.kt:41–51` 的 `setIndex` 已调用 `transitionCatalog`，但 `FavoritesScreen.kt:318–326` 的日期和末页按钮仍直接 `copy(jumpTo/prev/next)`。日期是 seek=原日期、prev=null、next=`2`；末页是 seek=null、prev=`1-0`、next=null。应让这两个按钮消费现有 Rust 转换，并保留创建新 builder 对象以触发 refresh 的方式。收藏末页的 `1-0` 与普通画廊末页的 `1` 不同。

**快速搜索创建**：`GalleryListScreen.kt:285–317` 仍负责禁止图片搜索保存、首个可见 gid 的 wrapping `+1`、相同查询下的进度判重、输入 trim/非空、可选 `@进度` 后缀、最终名称判重及 position=list.size。已有 `library::plan_quick_search_match` 只比较查询字段，`catalog_state::SavedProgress` 只取后缀，导入计划也不等同于创建计划。适合做批量预检查与确认后创建两个阶段，复用现有字段比较，不再对每个 saved search 单独跨 JNI。

保持现有阶段：首个可见项/进度在弹窗前取得，确认时重新读取当前名称集合，数据库插入成功后才更新列表和 saveProgress。测试包含空名、Kotlin Unicode trim、多个 `@`、非数字/溢出后缀、Long 溢出、相同查询不同进度及弹窗期间列表变化。

**标签 UI 校验和配置菜单**：`DownloadsScreen.kt:278–282,465–469` 的 blank、与本地化 defaultName 相等、名称存在等错误选择还在 Kotlin；`295–317` 的默认标签设置菜单也仍计算索引。这里有“让我选择 + 默认 + 其他标签”两项前缀，运行时 startDownload 的菜单只有一个默认项，不能复用一个未区分上下文的 index 偏移。当前设置菜单把找不到的默认标签映射到第 1 项（indexOfFirst=-1 后 +2），应先冻结此行为。Rust 返回错误码和设置意图，资源文本与 DataStore 写入保留。

### 2. 排序位置规则可共享，但不能全列表重排

`DownloadsScreen.kt:353–358,491–501`：拖动期间用 UI index-2 移动标签；结束时只把起点至终点闭区间内的行 position 写成当前列表索引。`GalleryListScreen.kt:336–338,427–437` 对快速搜索做同样的局部重编号，但没有两项 UI 前缀。`371–377` 的快速搜索删除仍在数据库返回后，按列表后缀逐项 position--。

可抽出无 UI 类型的移动/局部位置计划，并复用 `label_rules` 已有删除后缀规则。需覆盖向前/向后拖动、回到原位（不写库）、fromIndex=-1、稀疏/重复 position、同名不同 id、Int 溢出及拖动中列表变化。保持未受影响行的原 position；`EhDB.deleteQuickSearch:255–258` 的 SQL fill 继续按数据库 position 执行，不用列表索引替代它。触觉反馈、拖动库回调、Compose 状态和 launchIO 保留。

### 3. 批量收藏的跨存储选择仍可迁移

`FavoritesScreen.kt:339–385` 仍选择本地删除、云端删除、本地到云端、云端到本地、云端分类间移动及同类 no-op。Rust 现有 favorite_mutation 只接收已经选好的 single/bulk HTTP 请求，`library` 和本批 `entry_actions` 也没有覆盖这些多选路径。

必须保留的当前语义：

- 本地到云端先顺序执行 `EhEngine.addFavorites`，全部成功返回后才删本地；中途失败可能已有部分云端成功，本地记录保留。`EhEngine.kt:736–747` 的逐项请求及耗时后的 delay 顺序保持。
- 云端到本地只调用 `EhDB.putLocalFavorites`，保留云端收藏。云端到云端才调用 bulk move。
- 源分类在弹窗/确认之后读取；多选集合的 takeAndClear 时机在删除和移动路径中不同。保留同类 no-op、普通错误后的 refresh 以及原取消路径。
- 三种整数上下文不同：`GalleryInfo.favoriteSlot` 的 -2=未收藏、-1=本地；`FavListUrlBuilder.favCat` 的 -2=本地、-1=全部云端；HTTP mutation destination 的 -1=删除。建议闭合枚举或明确的输入种类，避免复用单项菜单换算产生含义混淆。

另一个可随批次合并的小规则是 `EhEngine.kt:728–732` 的本地收藏覆盖：仅当远端 favoriteSlot=NOT_FAVORITED 且本地存在时设为 LOCAL_FAVORITED。现在的 `gallery_enrichment` 只负责语言和元数据请求资格，未包含此规则。可用 Android 提供的存在性观察批量规划，保留当前短路查询和填充顺序；数据库查询批量化属于另一个优化选择。

验收应使用效果记录测试覆盖每种分支、部分 HTTP 成功后的失败、本地写入失败、同类选择和取消，再验证真实 JNI 的请求/计划。不要在语言迁移中引入自动补偿删除或请求并行化。

### 4. 恢复合并需先落实事务内的策略接口

`DownloadRecoveryDao.kt:163–206` 仍合并图库字段与 artist：正式记录非空白文本、非零分类/评分、非空 tags、正 pages 优先，正式 favoriteSlot 保留；artist 按旧记录+新记录顺序 trim、去空、稳定去重，结果为空时不重写。`DownloadRecoveryEhLookup.kt:147–150` 的 `artist:` 提取可一起规划；扫描直接提升路径目前传空 artist 列表（DownloadRecoveryCoordinator:297–303）。

`download_metadata` 的侧车补全策略保留重复 artist，缺失条件也不同，不能直接替代这里的合并。建议返回字段来源/artist 结果，Android 按计划构造原实体，保留原始 Float 位值和字符串；测试 NaN、正负零、null/空 tags、Unicode blank/trim 与顺序。

依赖方向是实施前的关键点：`core/data/build.gradle.kts` 依赖 core/common、Room/DataStore，没有 app JNI 依赖。可定义 core 层的同步策略接口，由应用提供 Rust 适配，并在现有 `@Transaction promoteVerified` 内、读取最新行后调用；具体 DAO 签名先经两变体 KSP 编译验证。身份、token、页数、路径所有权和活动下载检查仍在事务内，不能把已合并快照提前放到事务外。必须增加真实 Room 的冲突/失败回滚测试；当前已有恢复计划/目录测试，未找到针对该事务合并的专用测试文件。

### 5. 阅读器以离散交互计划为单位

上一轮列出的点击区域、反转、键盘、平移与页边界仍在 Kotlin。进一步核对确认：

- `ViewerNavigation.kt:33–49` 与五个 navigation 对象定义 0.33f/0.66f 区域、偏好缺省以及首个命中；`RectExtensions.kt:6–14` 负责水平/垂直反转。保持原 Float 常量，不能改成数学 1/3、2/3。
- `PagerViewer.kt:190–265` 在点击当下读取 viewport；NEXT/PREV 的 pan 方向受 RTL 影响，LEFT/RIGHT 也有独立映射；剩余距离严格大于 1f 才 pan。`KeyEventHandler.kt:15–27` 保留 KeyDown 和事件是否消费。
- `Utils.kt:44–64,74–107` 有缩放/对齐选择、页边界、webtoon 的 0.75 倍 viewport 滚动量；`DoubleTapZoom.kt:9–20` 有 enabled、未就绪、0.05f 阈值及初始 scale×2 选择。
- `SliderPagerDoubleSync.kt:29–78` 的可见页选择与 1-based 转换也可规划；snapshotFlow、drop(1)、collectLatest 和双向跟随切换的执行时序留在 Compose。

建议先迁区域/按键/单击 action，再单独处理缩放与滑块。配置变化时缓存区域，事件到来时用最新平台事实调用一次；每帧布局、Telephoto 坐标转换、pan/zoom/scroll 动画继续使用平台库。已有 reader schedule/fetch/memory/ads 规则继续复用。测试区域边缘、零尺寸、NaN、LTR/RTL/系统布局方向、页首尾和连续输入取消。

### 6. 缩略图要覆盖两个清理分支

`DownloadThumbInterceptor.kt:31–63` 的 key 已由 Rust 生成，剩余可迁的是流程选择。精确顺序是：必要时先更新图库 thumbKey；检查后缀；本地文件存在则先解码；本地 SuccessResult 时也会清理不同路径的旧 thumb.jpg 并直接返回；普通本地失败结果继续远端；远端成功且目录存在时尝试缓存写回，再清理旧 JPEG。直接抛出的异常与取消不等同于 ErrorResult，不能无条件当作远端 fallback。

适合由 Rust 返回分阶段 action，Android 回传本地/远端结果、目录和缓存观察。应测本地命中、解码错误结果、异常/取消、远端失败、缓存 miss、目录消失、相同后缀及清理顺序。保留现有覆盖写，避免变成先删再建。

### 其他候选与清理项

- 收藏分类快照：`FavoritesViewModel.kt:68–70` 分别写名称、数量和 wrapping Int sum；`Settings.kt:267–296` 按十个槽读取/写入默认值；`UserConfigParser.kt:12–14` 已用 Rust 解析，只留下设置应用。可扩展原应用 `settings` 的批量计划。要保留键缺失与类型/数值转换、长度错误、通知和写入阶段；是否改成一次 DataStore 事务应另行明确。本项不继续整合暂停中的 native 收藏策略。
- 排序偏好：`SortMode.kt:7–8,42–48` 的 flag 编解码和 `DownloadsScreen.kt:769–785` 的选项映射仍有纯规则。注意 group 判断是 `flag shr 4 == 1`，不是只检查某一 bit；非法字段回落默认，PAGES 值为 4。适合随目录规则合并。
- 恢复扫描收尾：`DownloadRecoveryCoordinator.kt:309–329` 仍拼接去重 PROMOTION_CONFLICT issues、合并 limit 标志、决定是否标记未见来源及计算 pending。可扩展恢复策略，但必须保持三种 limit 任一命中即不做缺失标记，并区分此扫描失败语义与已迁 EH lookup 的 LOCAL_PROMOTION_FAILED。
- `SessionController.kt:96–115,136–154,192–195` 的代次/一次性完成判定技术上可 reducer 化；它的锁、两次检查、规划前消耗 token 与 NonCancellable 提交紧密耦合，优先级低于前述模块。保留 Android 凭据、Cookie、并发与平台效果。
- 清理项：`SortMode.comparator()` 未找到调用；`DownloadsFilterState.take` 和 `DownloadsFilterMode.take` 形成旧过滤链，未找到从生产入口使用它的调用。当前入口是 DownloadManager:399–411 的 Rust sort 和 DownloadsScreen:206 的 Rust filter。它们应列为移除冗余实现的候选，不算一项新的算法迁移；仍被使用的模式 enum/序列化和 flag API 需要保留。
- 平台侧继续保留 Compose/UI、Room/DataStore、SAF、Coil/Bitmap、HTTP 执行、ICU 自定义正则和系统服务。小型显示格式、单个布尔切换或资源映射只有合并成有意义的计划时才值得新增 JNI。

建议下一批先做：收藏页导航补接、快速搜索创建/判重、两类列表位置计划及标签设置菜单。之后单独做批量收藏；恢复合并先验证事务接口，再推进 reader 与缩略图。

本轮未运行构建/测试，也未更改业务源码。开始快照 `/tmp/ehviewer-round3-audit-before.json` 包含 1,796 个既有文件；结束核对确认仅两份既有审查/进度文档发生变化，0 新增、0 删除，app/core/native/scripts/.github 下 1,736 个既有文件全部未变（含 native 414 个）。结果记录于 `/tmp/ehviewer-round3-audit-check.json`，前两批未提交实现完整保留；`git diff --check` 通过。


## 2026-10-29：第三批获批实施记录

用户批准继续后，实施上表前三组：目录/快速搜索入口、列表位置计划、批量收藏与本地状态覆盖。恢复事务合并、阅读器交互和缩略图流程不在本批，`native/` 保持暂停。新增三个 `eh_domain` owner、三个 JNI V1 导出，沿用既有 `catalog_state`、`library`、`label_rules` 和 HTTP favorite mutation 计划。

- **收藏页导航**：日期和末页按钮通过 Rust 返回新 builder，保留 SavedState/refresh 的对象替换方式，末页仍为 `prev=1-0`。
- **快速搜索**：先由 Rust 判定是否允许创建，再读取首个可见 gid；一次快照批量复用既有七字段比较，解析最后一个 `@` 后的进度并选出首个重复项。确认弹窗时使用最新名称列表进行 trim、后缀添加、判重和追加位置规划。Android 提供 `Character.digit` 与 `Char.isWhitespace` 分类观察；Rust 保持符号、Long 范围/溢出和原始 UTF-16 名称。数据库成功后再修改列表和 saveProgress。
- **标签编辑和设置菜单**：Rust 选择 blank / 本地化保留名称 / 重名错误，保留顺序且不自动 trim 标签。设置菜单有“让我选择、默认”两项前缀，普通标签菜单维持一个前缀；缺失默认标签的初始选中行仍为 1。选择“让我选择”只关闭 hasDefault，不清空原标签值。
- **位置计划**：`list_order` 共享数据行与 UI header 的索引转换、拖动结束的局部重编号和删除后缀减一。标签删除也复用同一 Rust 后缀函数。完整计划验证后再修改列表；原有 SQL fill、局部写库范围、拖动反馈与 launchIO 保留。
- **批量收藏**：Rust 区分本地删除、云端删除、上传后删除本地、保存本地副本、云端移动及同类 no-op。Android 效果执行器保留顺序和取消传播；上传部分成功后失败也不会删除本地，云转本地继续保留云端。确认/弹窗之后才读取源分类，多选清空与普通错误后刷新的位置不变。
- **本地收藏覆盖**：Rust 返回是否需要查本地及命中时的写入意图；平台逐条、按原顺序延迟查询。已收藏的行跳过 Room 查询，查询期间发生的字段变化按原短路语义处理，后续语言填充顺序保持。

跨语言回归发现 Kotlin CBOR 对显式 null 查询对象的编码与 Rust Option 不一致；创建阶段现省略该不适用字段，Check 阶段仍传完整查询。Rust/Kotlin/Java 共用四套新黄金样本，旧样本保留。测试增加完整 BMP 字符分类对照、signed Long 边界、raw surrogate 名称、两种 header 的移动矩阵、稀疏位置、无效计划，以及上传失败/取消和本地写入失败的效果顺序。

最终验证（冻结后的当前工作树）：

| 检查 | 结果 |
| --- | --- |
| Rust workspace / all features / integration / doctests | 620 项通过，0 失败、0 忽略 |
| JVM defaultDebug / marshmallowDebug | 各 458 项通过，0 失败、0 跳过；使用本轮重新构建并链接的 host JNI 库 |
| 新 owner 独立覆盖率 | quick_search 150/150 行、18/18 函数；list_order 63/63、7/7；favorite_batch 20/20、2/2，全部 100% |
| 受影响的旧 owner 覆盖率 | label_rules 91/93 行、8/8 函数；catalog_state 88/88、10/10；library 215/215、17/17，均通过原 95% 门槛 |
| JNI / 字符与效果差分 | 四套新 CBOR 黄金样本在 Rust/Kotlin/Java 间一致；完整 65,536 个 BMP code unit 的 JVM 数字分类与后缀检查；Java 183 组批量收藏/menu/overlay 对照；非法缓冲区、大小和 CBOR 拒绝；前两批全部 host JNI fixtures 通过 |
| Android 测试 | 两变体 AndroidTest Kotlin 编译通过；新增设备用例涵盖三个新 JNI 及扩展的导航/标签规则；adb 无设备，未执行设备运行测试 |
| APK / JNI / ABI | 独立 Debug 打包产出 default + marshmallow × ARM64 + x86_64 四个分包；精确导出各 130 / 129（含 JNI_OnLoad），退役符号阴性；包内库与 stripped 库逐字节一致，ELF ABI、签名、16KB zipalign 通过 |
| Rust 静态检查 | host workspace/all-features/all-targets Clippy `-D warnings`、rustdoc `-D warnings` 通过 |
| Android lint / 格式 / CI | lintMarshmallowRelease 0 错误、2 个既有警告（Typos、ChromeOsAbiSupport）；全部变更 Kotlin 的 scoped Spotless 通过；42 项脚本测试、actionlint、shellcheck、ruff、`git diff --check` 通过 |

APK 验收曾发现同次调用中的 lint…Release 触发仓库已有的 universal 包开关；随后以独立 assembleDefaultDebug/assembleMarshmallowDebug 命令重建，并按最终 output-metadata 的四个 ABI 分包验收。构建脚本未改动，初始日志保留。

源码核对：本批修改 25 个既有文件、新增 26 个文件，没有移除既有文件；`native/` 的 414 个既有文件未变，前两批工作保留。最终 1,762 个源码/配置文件的冻结哈希在 APK 验证时逐一核对通过。原 MainActivity/KtorExtensionsTest 全局格式遗留问题未触及。

证据位于 `/tmp/ehviewer-round3-{rust-tests,jni,coverage,clippy,rustdoc,script-tests}.log`、`/tmp/ehviewer-round3-quick-coverage-final.log`、`/tmp/ehviewer-round3-android-final.log`、`/tmp/ehviewer-round3-apk-final.log`、`/tmp/ehviewer-round3-artifacts.json`、`/tmp/ehviewer-round3-source-check.json` 和 `/tmp/ehviewer-round3-frozen-source.json`。复现入口 `scripts/test_followup_jni.sh` / `scripts/check_followup_coverage.py` 已包含新 owner；原有 42 个 CI coverage mask 同步维护。

本批未提交 Git、未安装 APK。下一阶段仍是恢复事务合并、阅读器交互和缩略图流程。


## 2026-10-29：第四轮剩余职责审查（第三批之后）

基线为第三批已实现、尚未提交的工作树，HEAD 仍为 `253852b49`。本轮只做源码和调用点审查，重点复核恢复、阅读器和缩略图，并扩展检查详情页预览、评论/投票以及设置。前三批实现均保留，`native/` 不恢复开发。

剩余可迁移内容主要是事务内的数据选择规则、页面状态规划和效果执行前的决策。建议顺序如下：

| 优先级 | 模块 | Rust 可负责的内容 | 平台保留的职责 |
| --- | --- | --- | --- |
| 1 | 恢复入库与扫描持久化 | 正式元数据合并、artist 归并、候选字段保留、issues/status/扫描收尾计划 | Room 事务、事务内重验、实际读写、取消和时钟 |
| 2 | 阅读器入口与离散交互 | 来源/起始页/模式选择，区域命中、按键、RTL、平移与翻页选择 | 文件/缓存观察、PageLoader 生命周期、Compose、Telephoto 动画 |
| 3 | 下载缩略图流程 | 本地尝试、远端回退、缓存写回和旧文件清理的分阶段意图 | Coil、Room、DiskCache、文件句柄和 I/O |
| 4 | 详情预览整数分页（本轮新增确认） | 请求窗口、缺失预览页去重、返回顺序、前后 key 和占位数量 | Paging 类型、预览对象/cache map、HTTP 并发与缩略图预取 |
| 5 | 评论/标签动作与评分显示（本轮补充） | 菜单可用项、投票/取消提示、票数文本拆分、评论锚点、评分档位 | 资源文本、Html/Spanned、对话框、HTTP 和权威响应应用 |
| 6 | 设置和轻量显示规则 | 收藏十分类默认值/计数和、排序 flag、归档分组/标签标记 | DataStore 时序/通知、资源与布局 |

### 1. 恢复要区分三类合并规则

**正式数据合并**仍位于 `core/data/.../DownloadRecoveryDao.kt:163–206`。已有正式记录时，保留 gid/token/favoriteSlot，按非空白文本、非零分类/评分、非空 tags、正 pages 决定旧值是否优先；无正式记录时直接采用 incoming。artist 在图库/下载/path 写入之后读取，按旧+新顺序 trim、去空、稳定去重，空结果不重写。

**扫描候选持久化**仍位于 `DownloadRecoveryCoordinator.kt:214–260`：KEEP_EXISTING 时移除旧候选；其他候选对 gid/token/pageToken/pageIndex/title/declaredPages 做 null 回退，保留旧 titleJpn 与 createdAt；路径观察还会覆盖 PATH_CONFLICT 并追加 ANDROID_PATH_CONFLICT。这里的空字符串是非 null 值，和正式合并的“非空白优先”不同。已有侧车 `download_metadata` 又保留重复 artist，三者应各有明确操作种类，共享字符串/索引工具即可。

**扫描收尾**仍位于 `DownloadRecoveryCoordinator.kt:309–333`：普通提升失败写入 PATH_CONFLICT/PROMOTION_CONFLICT；取消直接传播；三个 limit 标志任一成立就跳过未见来源的 SOURCE_MISSING 更新；pending 只统计本 root 下非 SOURCE_MISSING 的候选。issues 的 split/filter/distinct/join 还在 Kotlin，其中过滤空白片段不等于 trim 每一个非空片段。

实施路径建议分两步。先迁应用层候选行/收尾计划；正式合并则由 core/data 定义同步策略接口，app 提供 Rust 实现，在当前 `@Transaction` 内的原调用位置执行。`core/data/build.gradle.kts` 没有 app JNI 依赖，接入首先验证 KSP 两变体编译。保留事务内身份、token、页数、路径占用和活动下载重验，以及字段写入→artist 读取/写入→删候选→重载结果的顺序。

本轮还确认 `DownloadRecoveryEhLookup.kt:249–274` 依赖这些 Room 检查的固定异常文本生成封闭 promotion signals。调整策略接口时应保持该映射契约，并为策略传输错误保留可区分的失败路径。回归重点是过期候选、同路径争用、活动下载、每个写入点失败时整笔回滚、NaN/正负零、Unicode blank/trim 和三类合并规则差异。

### 2. 阅读器先迁入口/模式，再迁离散事件

本轮新增定位的入口是 `ReaderScreen.kt:421–461`。Gallery 参数先在 page=-1 时读取保存进度，再查托管归档；`DownloadManager.kt:933–936` 按 `$gid.cbz`、`$gid.zip` 顺序查找。命中归档后使用 archive loader，打开失败会走当前错误路径；未命中才选 EhPageLoader。EhPageLoader 继续使用 SpiderQueen 的本地/缓存/网络读取。显式 Archive/Directory 参数分别选现有 loader；外部归档的密码对话框和密码内容保留在 Android。`ArchivePageLoader.kt:47` 的排序开关也可以随入口计划合并。

模式规则位于 `ReaderScreen.kt:207–220` 和 `ReadingModeType.kt:28–40`：未知偏好回落 DEFAULT，DEFAULT 根据 case-sensitive `endsWith("webtoon")` 观察选择 WEBTOON 或 RIGHT_TO_LEFT。不要把这个现有后缀匹配收紧成另一套 tag grammar。

离散交互仍在 ViewerNavigation、KeyEventHandler、PagerViewer、WebtoonViewer、Utils 和 SliderPagerDoubleSync。除上一轮的 Float 区域端点、反转、1f 平移阈值和 0.05f 双击阈值外，本轮进一步确认：

- `ReaderScreen.kt:245–253` 在工具栏显示时禁用音量导航；方向/Page 键沿原逻辑处理。
- 按键回调直接翻页或滚动；点击区域才进入 PagerViewer 的“先尝试 pan，再翻页”流程。事件种类应进入计划，保留各自路径。
- 垂直 Pager 向 PageContainer 传入 isRtl=false；水平 reverseLayout 取阅读方向与系统 LayoutDirection 的 xor，不能仅使用其中一个方向。
- 点击时读取最新 viewport，模式/区域计划随当前偏好变化；异步打开开始时的一次快照不足以承担后续交互状态。

优先采用一次离散事件返回一个完整动作的边界。区域配置可缓存；布局、坐标转换、每帧动画、Canvas 和 coroutine cancellation 仍交给平台。缩放/对齐、亮度/滤镜可另作偏好变化时的批量计划，避免每次绘制跨 JNI。回归包含模式缺省、归档优先级、显式页码跳过数据库读取、归档失败、工具栏门控、区域边缘、NaN/零尺寸、RTL 和页首尾。

### 3. 缩略图流程需要精确的分阶段计划

`DownloadThumbInterceptor.kt:31–63` 和 `DiskCacheScoped.kt:31` 仍包含完整的分支选择。图库 thumbKey 的对象/数据库更新发生在后缀检查与文件尝试之前；本地 SuccessResult 清理不同路径的旧 JPEG 并立即返回；普通本地失败结果继续远端，异常/取消保留原传播路径。

本轮确认的细节是：远端成功且目录存在后，`thumbCache.read` 在缓存 miss 时返回 null，但其后的旧 JPEG 删除仍执行；缓存写回抛异常则不会执行后面的删除。memoryCacheKey 仍要求非 null。Rust 计划应表达这些阶段和条件，不能把“缓存写回确实发生”添加为既有清理规则的前置条件。

测试需要记录图库更新、本地读取、远端请求、缓存观察、覆盖写、删除与返回的顺序，覆盖两个成功分支、cache miss、写失败、同名 thumb.jpg、目录消失及取消。路径与文件对象保持平台所有权，复用已迁 cache key/文件规则。

### 4. 详情预览分页是独立的可迁移模块

入口为 `GalleryDetailContent.kt:861–911` 与 `PagingUtils.kt:17–30`。当前 Kotlin 计算：refresh anchor 裁剪；Prepend/Append/Refresh 的 offset 和 limit；end 上界；缓存缺失位置到 HTTP 预览页号的稳定去重；最终结果顺序、prev/next key、itemsBefore/itemsAfter。

已有 `catalog_state` 处理列表 cursor/seek 和排行榜；已有 `spider_info::preview_tokens` 处理下载侧 token 查询与增量。本项可复用 URL/HTML parser，但需要单独的 UI 分页计划。建议“加载前窗口/缺失页计划 + 响应消费后返回窗口”两个阶段，每次 load 批量调用。Android 保持 previewPagesMap（初始重复 position 后值覆盖前值）、当前 parMap 并发、响应消费顺序及缩略图预取。

测试重点：空/部分缓存、重复 position、向前不足一页、刷新 key 越界、末页、空范围、Int 溢出、pageSize=0 的失败边界、HTTP 失败，以及成功响应后仍缺页时原有 requireNotNull 路径。不要用默认条目填补缺页或顺手改变缓存寿命。

### 5. 评论、标签动作和评分可合批

`GalleryCommentsScreen.kt:258–267,369–421,437–442` 仍负责投票状态文本按逗号/末尾空格拆分、菜单项资格与顺序、投票/取消提示及 `#c` 评论 id 到首个列表位置的映射。可以扩展 comment_display / 动作规划，复用现有 JVM 字符观察与 raw UTF-16 契约。

要保留服务器刷新作为权威数据来源：评论投票成功后先刷新评论，刷新也成功才显示成功提示；第一步成功而刷新失败仍走当前失败提示。标签投票则使用服务器返回 tagGroups 重建详情（GalleryDetailScreen:150–159），不新增客户端猜测票态。菜单策略可以返回动作/消息种类，Html/Spanned、资源文字与网络执行保留。

评分显示在 `GalleryDetailContent.kt:719–746,839–851`，现有显示档位用 `(rating*2).roundToInt()`；请求 planner 的 `net/gallery/request.rs:419–420` 则用 ceil。它们承担不同职责，应分别保留取整契约，覆盖半步边界、超范围和非有限值。适合一次计算显示/动作计划，不单独为每次 Compose 重组增加 JNI。

### 6. 其他剩余项与清理范围

- 收藏分类：Settings:73–75,267–296 的十槽默认值、数组长度与计数；FavoritesViewModel:68–70 的三步设置应用和 Int sum。可迁纯数据计划，DataStore 的写入/通知阶段保持原状；一次事务改写属于单独行为变更。native 收藏快照集成继续暂停。
- 排序设置：SortMode:7–8,42–48 的 flag 编解码仍在 Kotlin，保留 `flag shr 4 == 1`、PAGES=4 与非法字段缺省。排序算法本身已经使用 Rust。
- 归档显示：ArchiveList:41,67–85 的 H@H 分组、org 标签以及普通下载 `Free!` / H@H `Free` 两种标记，可随详情页批量计划处理；资源文字和展示布局保留。
- SessionController 的请求代次/一次性完成仍可做纯状态转换，但两次检查、规划前消耗 token、锁与 NonCancellable 顺序使其收益小于上述候选，维持后续优先级。
- 旧 SortMode.comparator、DownloadsFilterState.take/DownloadsFilterMode.take 链本轮仍未找到生产入口调用，继续列为代码清理候选；仍使用的 enum/flag/序列化 API 保留。

建议下一批优先实施前三组，并在阅读器内先落地来源/模式和离散导航；整数预览分页作为紧随其后的独立批次。已完成的解析、URL、下载队列、排序过滤、归档解码和批量收藏规则继续复用。Compose、Room/DataStore 引擎、SAF、HTTP 执行、Coil/Bitmap、ICU 用户正则和系统服务保持当前职责。

本轮为静态审查，仅更新两份既有审查/进度文档，没有新增或删除文件，未运行构建/测试。开始快照 `/tmp/ehviewer-round4-audit-before.json` 包含 1,822 个文件；app/core/native/scripts/.github 下 1,762 个源码/配置文件全部保持原样（含 native 414 个），前三批未提交实现完整保留。最终差异记录于 `/tmp/ehviewer-round4-audit-check.json`，`git diff --check` 通过。

## 2026-10-29：第四批获批实施记录

用户批准继续后，落实上一轮前三组：恢复入库/扫描持久化、阅读器入口/模式/离散导航、下载缩略图阶段决策。新增 `recovery_persistence`、`reader_navigation`、`thumbnail_flow` 三个 domain owner 和三个 JNI V1 导出。既有恢复检查、reader_policy 的解码/下载状态规则、cache key、归档读取继续复用；完整 Rust 应用与 `native/` 的 F0–F8 仍暂停。

### 恢复事务与候选行

- core/data 新增同步 `RecoveryMergePolicy` 接口，由 app 注入 `RustRecoveryMergePolicy`。KSP 生成的 `DownloadRecoveryDao_Impl.promoteVerified` 继续用 `performInTransactionSuspending` 包住完整方法，并将策略参数传给原 DAO 实现；core/data 没有引入 app/JNI 依赖。
- 原有候选身份、token、页数、路径占用与活动下载重验及其异常文本保留。正式元数据选择在原 gallery upsert 前执行；artist 仍在 gallery/download/dirname 写入之后读取并合并，随后才删候选、重载结果。策略失败沿 Room 的异常路径回滚。
- Rust 返回字段来源选择，不往返转换图库字符串、身份或评分值。已有 gid/token/favoriteSlot 保留；非空白旧文本、非零 category/rating、非空 tags、正 pages 优先。rating 使用原始 Float bits 作观察，因此 NaN、正负零不改变原规则。Android 提供自己的 Unicode blank/whitespace 分类。
- artist 使用原始 UTF-16LE，Rust 返回原名称的 trim 区间，按旧+新顺序去空、稳定去重；空合并结果继续跳过 artist 重写。候选 staging 另用 null 存在性选择，空字符串/零值仍可覆盖旧字段；保留旧 titleJpn/创建时间和 Room 的 sourceKey 再检查。
- KEEP_EXISTING、issues 去重、路径阻塞状态、promotion failure 的 issue 追加、三个 limit 标志和 pending 计数移入 Rust。failure 过滤空白片段但不 trim 非空片段；时钟调用、取消传播和数据库执行次序保持。

### 阅读器与缩略图

- Gallery 入口在 page=-1 时先读保存进度，再查托管归档；显式页码跳过进度读取。Rust 选择 archive/EhPageLoader，Android 保留原 cbz/zip 查找顺序、显式 Archive/Directory 类型分发、打开/密码与 loader 生命周期；托管归档打开失败仍走现有错误路径。
- 默认阅读模式、case-sensitive webtoon 后缀、五种点击区域及反转、首次区域命中、RTL 平移/翻页、音量/方向键动作、页首尾与 wrapping Int、归档排序开关和 0.05f 双击阈值进入 Rust。Float 几何使用原始 bits；点击继续读取最新 viewport，按键继续直接翻页/滚动，点击才按 `remaining > 1f` 尝试 pan。
- 保留垂直 Pager 的非 RTL 内容及水平阅读方向与系统方向 xor。按有效导航模式复用五个稳定对象，使默认模式与等效显式模式不会额外触发 Compose 导航提示。旧五个区域定义文件及仅被它们使用的 Rect 反转文件删除，规则集中到 Rust；渲染、布局、连续滚动状态、缩放计算和动画仍由平台执行。
- 缩略图按本地尝试、本地结果、远端结果、缓存写回后清理分阶段规划。图库 thumbKey 更新仍先于目录构造和后缀检查；本地成功清理旧 JPEG 后返回；普通本地失败回退远端。远端成功且目录存在才读缓存并覆盖写，cache miss 仍到达旧 JPEG 清理；写入失败/取消不会推进到清理。同路径 JPEG 保留，不删除后重建 SAF 文件。

### 验证与交付边界

| 检查 | 结果 |
| --- | --- |
| Rust workspace / all features / integration / doctests | 634 项通过，0 失败、0 忽略 |
| JVM defaultDebug / marshmallowDebug | 各 474 项通过，0 失败、0 跳过；使用当前源码重建的 host JNI 库 |
| 独立覆盖率 | recovery_persistence 200/200 行、19/19 函数；reader_navigation 172/172 行、13/13 函数；thumbnail_flow 34/34 行、5/5 函数，全部 100% |
| JNI / 兼容性回归 | 14 个 Java host fixtures 通过；两套新 CBOR 黄金样本在 Rust/Kotlin/Java 间一致；Unicode 空白、原始 surrogate、Float 边界、全部导航模式/反转、按键矩阵、效果顺序与取消/失败路径通过 |
| Room / AndroidTest | 两变体设备测试源码编译通过；新增 7 个设备测试，覆盖实际 Room 各写入点与最终重载失败的回滚、策略失败/取消、过期候选与路径争用，以及阅读器/缩略图 JNI。adb 无设备，这些设备用例及真实 UI/SAF 流程尚未运行 |
| APK / JNI / ABI | default + marshmallow × ARM64 + x86_64 四个 Debug 分包通过；精确导出分别 133 / 132（含 JNI_OnLoad），退役符号阴性；包内库与 stripped 库逐字节一致，ELF ABI、签名、16KB zipalign 通过 |
| Rust 静态检查 | host workspace/all-features/all-targets Clippy `-D warnings`、rustdoc `-D warnings` 通过 |
| Android lint | lintMarshmallowRelease 0 错误、2 个既有警告（arrays.xml:169 Typos、build.gradle.kts:37 ChromeOsAbiSupport） |
| 格式 / CI | app 与 core/data 的变更 Kotlin scoped Spotless 通过；42 项脚本测试、actionlint、shellcheck、ruff、`git diff --check` 通过；15 个 follow-up owner 和全部 42 个原 CI coverage mask 已同步 |

首次打包遇到 core/data 的 DEX 增量缓存引用旧工作区路径；清理该模块生成目录并以 `--no-build-cache` 重跑后，完整 JVM/设备源码编译/四包构建通过。保留首次失败日志，未改构建脚本。Android lint 已单独通过，避免既有 Release 任务名开关影响 Debug 分包配置。

源码冻结记录包含 app/core/native/scripts/.github 下 1,781 个文件，APK 验证时全部哈希匹配。相对本批开始快照，业务/验证范围修改 19 个既有文件、新增 25 个文件、移除上述 6 个导航旧文件；另同步两份文档。`native/` 的 414 个文件未变，前三批实现保留。测试生成的 Python bytecode 已清除，后续脚本检查关闭 bytecode 写入；既有 MainActivity/KtorExtensionsTest 全局格式遗留未触及。

证据：`/tmp/ehviewer-round4-{rust-tests,jni,coverage,clippy,rustdoc,script-tests}.log`、`/tmp/ehviewer-round4-reader-coverage-final.log`、`/tmp/ehviewer-round4-android-rebuild.log`、`/tmp/ehviewer-round4-lint.log`、`/tmp/ehviewer-round4-artifacts.json`、`/tmp/ehviewer-round4-frozen-source.json` 与 `/tmp/ehviewer-round4-source-check.json`。复现入口为 `scripts/test_followup_jni.sh` 和 `scripts/check_followup_coverage.py`。本批变更留在工作区，未安装 APK。

后续优先级为详情预览整数分页、评论/标签动作与评分显示、设置和轻量显示规则。阅读器连续布局/动画、Coil/SAF/Room 引擎继续保留平台职责；`native/` 不恢复开发。
