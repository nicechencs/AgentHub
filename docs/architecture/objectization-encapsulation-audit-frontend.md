---
title: 对象化与封装审查：前端页面与运行时
type: explanation
status: current
owner: maintainers
updated: 2026-08-26
---

# 对象化与封装审查：前端页面与运行时

本分册记录前端生产源码的逐文件复核结果，包含上一轮排除的页面、API、runtime、连接流程，以及公共组件、配置和工具层。总览见[对象化与封装审查](objectization-encapsulation-audit.md)。

## 覆盖范围

本轮以当前磁盘内容为准逐项检查 `src` 下全部 `.ts/.tsx/.css/.d.ts`：共 587 个文件，其中生产源码 392 个、测试/fixture 195 个。重点补查 147 个生产文件：`src/pages/**` 98 个、`src/lib/api/**` 23 个、`src/app/runtime/**` 13 个、`src/components/connect/**` 6 个、`src/lib/connect-flow/**` 7 个。其余 245 个生产文件也逐项复核。

未列入问题表的页面、API façade、连接流程、布局组件、基础组件、配置、样式、backend contract、provider detection、国际化和工具文件，均为“已审查 / 未发现新增对象化问题”。

## 新增问题

### O-49｜Agent 卡片 Hook 暴露内部状态 setter

- **位置：** `src/pages/agents/use-agent-card-lifecycle.ts:136,386`
- **问题：** Hook 内部拥有任务、确认框、环境面板等状态，却返回 `setTask`、`setConfirmDialog`、`setConfirmName`、`setShowEnvPanel`、`setEnvAutoStart`，调用方可以直接写入内部状态。
- **建议：** 只返回 `openUninstallConfirm`、`closeEnvironmentPanel`、`dismissTask` 等语义化命令，让任务状态由生命周期动作驱动。
- **影响：** 后续组件可绕过生命周期约束制造非法状态。

### O-50｜项目 Hook 暴露共享缓存写入口

- **位置：** `src/lib/hooks/useProjects.ts:326,376`
- **问题：** `useAgentProjectList` 返回 `setData`，调用方可以直接修改组件状态、模块级 `lists`、`writeClock` 并通知订阅者，绕过统一请求排序和失效代数。
- **建议：** 改为 `replaceProjectListFromMutation`、`removeProject`、`invalidateProjects` 等受限操作，或完全隐藏缓存写入。
- **影响：** 多个页面并用项目缓存时，局部状态和共享读模型可能不一致。

### O-51｜Agent 状态 Store 缺少 reset 后的异步隔离

- **位置：** `src/app/runtime/agent-status-store.ts`
- **状态：已处理**
- **问题：** reset 只清空 `inflight`，没有 epoch；旧 `listAgents()` 请求完成后仍可把旧 backend 数据写回新 Store。
- **当前：** 与连接池 / 票夹一样按 epoch 丢弃过期写回；`finally` 只清理自己那次 inflight。
- **建议：** 为请求记录开始时的 epoch，在所有成功和失败 continuation 写回前校验 epoch。
- **影响：** 切换 backend、重新初始化或测试 reset 后可能出现旧数据污染。

### O-52｜Agent Catalog Store 缺少 reset 后的异步隔离

- **位置：** `src/app/runtime/agent-catalog-store.ts`
- **状态：已处理**
- **问题：** reset 后旧 catalog 请求仍可执行 `applyAgentCatalog` 和 `setSnapshot`，并改变全局 Agent 集合。
- **当前：** catalog 有 epoch；`setBackend` / `resetBackend` 会一起清空 catalog。测试 setup 在 reset 之后会重新 seed。
- **建议：** 增加 epoch；明确 `setBackend/resetBackend` 是否同时重置 catalog。
- **影响：** 新 backend 可能被旧 catalog 覆盖。

### O-53｜Runtime Context 与外部 Store 存在双重订阅模型

- **位置：** `src/app/runtime/AgentCatalogProvider.tsx:8,22`、`ConnectionPoolProvider.tsx:9,27`
- **问题：** Provider 创建 Context，但主要消费者通过 `useAgentCatalogOptional`、`useConnectionPool` 直接订阅外部 Store；同一状态存在两套访问路径。
- **建议：** 要么删除无实际作用的 Context/Provider，要么让强制 Hook 统一从 Context 读取，Optional Hook 再直接订阅 Store。
- **影响：** 状态访问模型重复，未来容易误判 Provider 与 Store 是否同步。

## 已确认合理的边界

- `src/lib/api/**` 目前保持薄委托，未发现新的直接调用底层 `invoke` 或持有内部 Store 的问题。
- `src/components/connect/**` 与 `src/lib/connect-flow/**` 的状态机、generation、确认锁和 plan fan-out 仍以流程对象/纯函数为主，未发现新的高置信问题。
- 公共 UI 组件、布局、样式和本地工具未发现把业务状态外泄或把业务行为错误放入通用组件的新增问题。
