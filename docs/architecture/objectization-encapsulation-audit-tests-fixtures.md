---
title: 对象化与封装审查：测试、Mock 与 Fixture
type: explanation
status: current
owner: maintainers
updated: 2026-08-27
---

# 对象化与封装审查：测试、Mock 与 Fixture

本分册记录测试、E2E、Mock、Fixture 和测试脚本的逐文件复核结果。它关注测试对象是否完整承载状态、辅助对象是否泄漏可变内部数据，以及 Mock 是否与生产对象模型漂移。总览见[对象化与封装审查](objectization-encapsulation-audit.md)。

## 覆盖范围

去重后约 400 个文件：前端测试及 `__tests__` 195 个、`src/dev/mocks/**` 49 个、`src/test/**` 1 个、E2E 5 个、Rust 测试源码和协议 fixture 154 个、测试脚本 7 个。所有文件均逐项纳入审查；未列入问题表的文件均为“已审查 / 未发现新的对象化或测试状态问题”。

## 新增问题

### O-54｜Skill Mock 状态没有按 Backend 实例隔离

- **位置：** `src/dev/mocks/skill.ts`、`src/dev/mocks/create-backend.ts`
- **状态：已处理**
- **问题：** `lastProjectionMode`、`mockState`、`mockPrivateSkills` 和 seeded random 为模块级状态，`createBackend()` 不重置。
- **当前：** `resetMockSkills()` 恢复种子目录并清空投影模式；`createBackend()` 会调用它。
- **建议：** 提供 `resetMockSkills()`，在 Backend factory/setup 调用；fixture 构造与运行时状态分离并按实例深拷贝。
- **影响：** 测试顺序敏感、并行不稳定，技能投影断言可能被前序测试污染。

### O-55｜Config Mock 把内部可变对象直接返回

- **位置：** `src/dev/mocks/config.ts`
- **状态：已处理**
- **问题：** `readAgentConfig()` 返回模块级 values 引用，`getAgentConfigSchema()` 也直接返回 schema；调用方可反向修改 Mock 内部状态。
- **当前：** schema、values、unknownNative 均返回深拷贝；apply 仍写入内部 store。
- **建议：** 读取接口统一返回深拷贝，schema、values 和 unknownNative 均不允许外部直接改写。
- **影响：** 测试辅助对象职责不清，并可能制造跨测试污染。

### O-56｜OAuth Mock 没有把 Agent/Provider 上下文建模进会话对象

- **位置：** `src/dev/mocks/account.ts`
- **状态：已处理**
- **问题：** `waitOAuth()`、`finishOAuth()` 和 `finishDeviceOAuth()` 使用固定 Agent/Provider 结果，state 只传字符串且不校验来源。
- **当前：** 按 `state` 保存 agent / provider / 流程；wait/finish/cancel/poll 查找会话，未知 state 会失败。
- **建议：** 建立包含 agent、providerKey、flow 的 OAuth session 对象，wait/finish/cancel 均按 session 查找和校验。
- **影响：** 多 Agent OAuth 测试可能误通过，Mock 无法发现串线。

### O-57｜Ticket Mock 与 Adapter Mock 重复维护来源分类

- **位置：** `src/dev/mocks/ticket.ts:70-180`、`src/dev/mocks/adapter/source-ticket.ts:15-180`
- **状态：已处理**
- **问题：** 两处分别维护多个 endpoint/preset 判断，字段输入模型也不同。
- **建议：** 抽出单一来源分类策略/fixture，两个 Mock 只消费统一分类结果。
- **影响：** Mock wallet 与 Mock adapter 可能对同一来源得出不同分类。

### O-58｜能力、安装渠道和配置 schema 是手写生产镜像

- **位置：** `src/lib/backend/contracts/catalog-mirror-contract.json`；`src/dev/mocks/capabilities.ts`、`src/dev/mocks/fixtures/install-catalog.ts`、`agent-catalog.ts`、`config.ts`，以及对应 Core catalog/config 测试
- **状态：部分处理**
- **问题：** Agent 集合、渠道、版本、字段和能力分散维护，已有契约测试仍主要验证局部字段。
- **当前：** Agent id、Capability 键、本机安装渠道 id、能力界面标签（`Capability::label()`）与 config schema 字段名对照 core / mock，缺项失败。未对照每格 capability reason 文案。
- **建议：** 从共享契约 JSON/生成产物获得 fixture，至少增加完整 Agent/channel/schema/capability 对照测试。
- **影响：** Vitest/E2E 可能通过，但 Tauri/Core 实际行为已漂移。

### O-59｜测试中的领域对象构造和 Fake Adapter 重复

- **位置：** `crates/agenthub-core/src/services/adapter_route_service/tests.rs`、`ticket_bind_service/tests.rs`、`ticket_read_service/tests.rs`
- **问题：** Provider、Account、Request、Adapter fixture 在多个测试模块重复构造，部分 fake adapter 也重复实现，默认字段不一致。
- **建议：** 建立测试专用 fixture builder，集中默认值，仅声明测试差异。
- **影响：** 测试对象容易偏离真实模型，字段变更时部分测试继续使用旧形状。

### O-60｜协议测试重复维护 Fixture Loader

- **位置：** `crates/agenthub-core/src/bridge/protocol/fixture_loader.rs`
- **状态：已处理**
- **问题：** 两个模块分别维护 fixture 名称到 `include_str!` 的映射。
- **当前：** `tests.rs` 与 `claude_codex_tests.rs` 共用一个 test-only loader。
- **建议：** 抽出测试专用 loader，或按协议域合并映射。
- **影响：** fixture 新增、重命名时容易只更新一处。

### O-61｜Usage Mock 数据不是 Factory 级状态

- **位置：** `src/dev/mocks/usage.ts`
- **状态：已处理**
- **问题：** usage records 在模块加载时一次生成，依赖初始化时钟，`createBackend()` 不 reset/reseed。
- **当前：** `resetMockUsage()` 按当前时间重新生成近 30 天数据；`createBackend()` 会调用它。窗口过滤仍用 `Date.now()`。
- **建议：** 使用固定时间基准，并让每个 Backend 实例生成独立数据或显式 reset。
- **影响：** 时间窗口断言随日期变化，不同测试共享同一批对象。

## 其他测试边界

- E2E 浏览器测试和前端 Vitest 的入口、setup、Playwright/Vitest 配置已检查，职责清晰，但当前没有 Tauri 与生产 backend 对等的 E2E 门禁；这是测试覆盖边界，不新增对象化问题。
- Mock Agent 的既有模块级状态问题已在总览 O-20 记录；本分册新增问题主要是 Skill、Config、OAuth、Ticket/Adapter 分类和 Usage fixture 的实例边界。
