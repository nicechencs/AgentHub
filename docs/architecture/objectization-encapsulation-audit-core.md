---
title: 对象化与封装审查：Core 剩余分区
type: explanation
status: current
owner: maintainers
updated: 2026-08-26
---

# 对象化与封装审查：Core 剩余分区

本分册记录 Rust Core 在既有 Provider、Account、Backup、Bridge Service 和前一轮 adapter/platform/usage 审查之外的逐文件复核结果。总览见[对象化与封装审查](objectization-encapsulation-audit.md)。

## 覆盖范围

本分册补查共 134 个文件：`services/**` 剩余范围 77 个、`storage/**` 40 个、`domain/**` 7 个、`adapter_control/**` 5 个、`presets/**` 1 个、`logging/**` 1 个，以及 Core 根文件 3 个。生产源码和测试源码均逐文件检查；未重复深入上一轮已完成的主 Service 文件，但相关测试文件纳入覆盖。Core 全部 468 个 `.rs/.sql` 文件中的 OAuth 12 个文件另列在 [OAuth 分册](objectization-encapsulation-audit-oauth.md)，其余目录由本分册与前一轮 adapter/platform 等分册覆盖。

## 新增问题

### O-45｜Agent 检测缓存是跨实例、跨 Registry 的全局状态

- **位置：** `crates/agenthub-core/src/services/agent_service.rs`
- **状态：已处理**
- **问题：** `AgentService` 有自己的 registry，但检测结果放在全局 `static CACHE`，没有绑定 Registry 身份或实例生命周期。
- **当前：** 检测结果缓存在实例上（同实例 clone 共享）；`invalidate_detect_cache()` 仍通过代数让所有实例失效，供安装/生命周期调用。
- **建议：** 把缓存放入 `AgentService` 实例，或以 Registry/catalog 版本作为缓存键，并保留显式失效入口。
- **影响：** 不同宿主、测试或 Registry 实例可能读到其他实例的检测结果。

### O-46｜Logging 绕过 Database 建立第二条 SQLite 访问路径

- **位置：** `crates/agenthub-core/src/logging/mod.rs:120-147`
- **状态：部分处理**
- **问题：** `load_log_prefs` 直接 `Connection::open(db_path)` 查询 settings，而其他设置读写经过 Database/SettingsService。
- **建议：** 启动时由统一 bootstrap 解析并传入 `LogConfig`；logging 不自行打开 SQLite。
- **影响：** 连接、锁等待、迁移和错误降级语义形成第二套实现。

### O-47｜Adapter 锁表没有随 profile 生命周期回收

- **位置：** `crates/agenthub-core/src/adapter_control/coordinator.rs:16-62`
- **状态：已处理**
- **问题：** `profiles` 和 `targets` 是永久增长的 HashMap，每个新 profile ID 都留下一个 Mutex 条目。
- **建议：** 使用可回收锁条目，或由 profile 注册/注销生命周期管理；固定 Agent 锁可保留枚举键。
- **影响：** 长期运行、批量导入或反复创建 profile 时，进程内锁表持续增长。

### O-48｜设置解析职责在 Database、SettingsService、Logging 间重复

- **位置：** `crates/agenthub-core/src/storage/mod.rs:157-214`、`crates/agenthub-core/src/services/settings_service.rs:22-100`、`crates/agenthub-core/src/logging/mod.rs:120-147`
- **状态：已处理**
- **问题：** 多处分别承担键名、默认值、校验和类型转换，logging 还自行查询原始字符串。
- **建议：** 由 SettingsService 成为设置值对象和校验 owner；Database 只做通用持久化；logging 只消费解析后的 `LogConfig`。
- **影响：** 默认值、合法范围和降级规则变更时需要同步多处。

## 逐文件结论

- `services/**` 剩余文件均已检查；除 `agent_service.rs`、`settings_service.rs` 和与既有 O-11/O-14/O-17/O-18/O-31/O-32 关联的文件外，未发现新的独立问题。
- `storage/**` 的 Repository 主要承载 SQL、行映射和必要的事务辅助；生产多表写入已有 ConnectionService 的 connection-scoped helper，未发现新的事务破坏点。已有风险归 O-02、O-11、O-12、O-32。
- `domain/protocol_graph/**` 保持纯表格和判定，I/O 与 Repository 读取仍在 Service 层，未发现新增问题。
- `adapter_control/**` 不直接持有 SQLite 或凭据，主要问题只有锁条目生命周期（O-47）。
- `presets/**` 是静态只读模板；`error.rs` 的错误结构和映射集中；均未发现新增对象化问题。
