---
title: Chat 第二批开发交接提示词
type: how-to
status: current
owner: maintainers
updated: 2026-09-05
---

# Chat 第二批开发交接提示词

将下面整段交给下一位开发 Agent。当前事实和测试结果只维护在 [B1 实施记录](../status/chat-codex-b1.md)，审查任务另用 [review 提示词](chat-review-handoff.md)。换电脑前需要取得包含这些文件的提交；本地提交不等于已经推送到远端。

```text
你接手 AgentHub 的 Chat 统一体验开发，请直接推进 B2 的实现、审查和验证，不要只返回设计方案。

目标：让未用过命令行的人也能操作不同 Agent。共用清晰的聊天界面，以每家真实能力适配；不克隆 VS Code 插件的私有 UI，不把按钮伪装成命令行文本。

先确认：
1. 阅读仓库 AGENTS.md、docs/proposals/chat-unified-experience.md、docs/status/chat-codex-b1.md。
2. 检查当前分支、commit、工作区 diff；日常工作在 dev，保留已有修改。B1 代码已在 c03acb540b0327cd3fb81a56060d930e7836585a，提示词初版在 357853762ac902808b6fac5636786abddefaa22c。还需取得随后补充测试记录的提交。db40f6fbf9c86a184707f7ff8259a5bac361bb24 只是任务最初基线，其后存在其他并行功能提交，不要全部算成 B1。
3. 结构问题优先 CodeGraph；不可用时定向读文件，不重复全仓探索。用户已允许 GPT-5.6 Terra/Luna 子 Agent；仅把文件不重叠的独立任务并行。

已实现，不要重新设计：
- 新空 Codex 会话走 ChatRuntime + app-server，旧消息会话与其他 Agent 保持 legacy。
- ChatPort runtimeSnapshot/Start/Reply/Steer/Cancel → src/lib/backend/tauri/chat.ts → src-tauri/src/commands/chat.rs → crates/agenthub-core/src/services/chat_runtime/。
- SQLite 先保存再提供快照，currentMessage 是同一读取中的完整回复；runtime agentChunk 不负责拼正文。不得通过 endsWith/startsWith 猜测消息重复。
- 页面按会话串行读取快照，轮询避免重叠；sequence、source version 和页面世代共同隔离晚到结果。
- 确认/回答与停止经串行会话处理；clientRequestId 的 pending/accepted/failed 记录不能把失败重试误报成功。
- 非重试错误进入终态并清除请求；willRetry 错误保持运行。终态释放进程，下一轮 resume。
- 00031 migration 前有包含 WAL 的一致性备份，备份失败不升级。当前迁移尚未发布，但不要在可能已用过 B1 的数据库上假定可以修改已应用的 00031；新持久化字段优先新增增量迁移。
- macOS / Codex 0.153.0 真实重开后记忆延续已通过，不代表三平台或全部真实交互通过。

B2 合并交付范围（Codex 优先）：
1. 会话模型和思考强度：从实际 model/list 读取，校验组合，作为会话/下一轮 turn/start 参数；不复用会修改本机供应商配置的旧 setChatModel。活动轮次不可悄悄改变配置，后端拒绝时保留原有效值。模型列表不是账号可调用的证明。
2. 统一操作菜单：菜单按钮与明确的 / 搜索态共用同一动作定义。优先新建/历史/复制/设置入口、只填写草稿的示例任务。普通路径和代码中的 / 不触发操作；未实现的原生命令不伪装成提示词。
3. 附件：先核对实际输入类型与模型能力。已观察到 localImage，普通文件/图片/音频分别验证，不能把所有附件拼成路径字符串就宣称支持。处理选择、移除、大小/类型、中文空格路径、读取失败、切换会话/模型后草稿保留。
4. Skills/插件：先用真实 skills/list、plugin/installed 做发现与准确状态。安装≠启用≠本轮加载≠可调用；显式 skill 输入必须核对稳定 ID/路径和真实效果。同名不可只按名称调用。完整安装/卸载/更新及 MCP 配置按方案 A17 后续单独定范围。

暂不扩大：
- 计划/协作模式曾仅在实验 schema 发现，必须重新验证协议与生效范围，不能默认上线。
- Claude 和其他 Agent 为 B3；ZCode 没有可验证接口时明确待验证，不承诺同等体验。
- 不实现凭据落盘加密、国产 OAuth 对接或 OAuth 转 API。所有 API Key 可分享，国产官方登录不可分享。
- 不修改用户级配置；invoke 仍仅在 src/lib/backend/tauri/；mock 不进入生产。
- 跨电脑开发可复现，不等于跨电脑迁移原生聊天已实现。

建议文件范围：
- core: services/chat_runtime/{types,mod,store}.rs 及对应测试、新增迁移。
- IPC/contracts: src-tauri/src/commands/chat.rs、src-tauri/src/lib.rs、src/lib/backend/contracts/chat-*.ts、tauri/chat.ts、lib/api/chat.ts、dev/mocks/chat.ts。
- UI: src/pages/chat/，必要 i18n。B2 的模型选择不要写入 use-chat-page-connection 原有供应商切换逻辑。
先定一份最小契约，再把后端与前端分配给不同 Agent；避免并行改同一共享类型。

测试和交付：
- 按 B1 实施记录使用固定命令；真实测试须显式 opt-in，它使用调用者现有 Codex 登录并生成原生会话，不能输出登录信息。
- 覆盖模型组合拒绝、活动轮次设置冻结、菜单一致性、附件草稿与失败恢复、扩展 unknown 状态，补适用 A11–A16。
- 合并补 B1 尚未完成的桌面真实验收：问答、文件审批、异常登录/版本、恢复失败、Windows/Linux。不能用 mock 替代真实证据。
- 提交时遇到的 6 个前端全量失败和 5 个 Rust 全量失败已记录在 B1 实施记录。用户要求这轮先不修，留到后续；先判断归属，不得把未排查项称为基线已证明，也不得通过删断言让检查变绿。
- 完成实现→独立 review→修复本批问题→验证→更新状态/交接。给出明确已完成/未完成、实际命令结果和下一步，不要求用户逐个确认按钮。
- B1 用户已要求提交，后续是否提交/推送按当前会话授权，不把本提示词当作自动发布授权。
```
