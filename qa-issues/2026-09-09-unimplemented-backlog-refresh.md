# 未实现 backlog 刷新（2026-09-09，#317/#318 之后）

对照 `origin/dev` tip `ef03bb36`。本页是工作区 QA 笔记，不是现行契约；跨模块事实以 `docs/STATUS.md` 已知边界为准。

#317 / #318 的界面与 Kimi / DSH 句子由仍打开的 #319 补进 STATUS，本刷新不改那些段落。

## tip 已合入（不要再开）

- #317：Kimi 自定义中继保留上游模型目录、叠写模型 id 收成一份、DSH `yaml_quote`。
- #318：Connections / 连接池登录详情按问题分块。

## 产品决定与已知边界（已写入 STATUS）

- Codex `request_user_input`（2026-09-09）：**等上游默认打开**；不启用实验开关；不做计划模式 B2。协议和界面已映射，产品会话默认发不出问答卡片。不再列为「要产品决定」。
- Codex computer use：Linux 真窗不可用（见 `qa-issues/CODEX-COMPUTER-USE-2026-09-09.md`）。STATUS 记为已知边界，不派工。

## 进行中（不要写成已落地）

- #320：DeepSeek 启动优先完整 npm 树（不是 PATH 上的残缺命令）。仍打开，未合入 tip。
- #319：STATUS / Chat 文档追上 #317/#318 与新空 Claude 图片。仍打开。

## 仍开着的验收（不从提案推导）

- Windows 上的 Codex 对话真窗尚未宣称。
- Kiro 企业 IdC / `profileArn` / `runtime.*.kiro.dev` 真窗未验。
