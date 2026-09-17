---
title: 可选本机命令护栏
type: proposal
status: proposed
owner: maintainers
audience: product owners and implementation agents
updated: 2026-09-15
---

# 可选本机命令护栏

**本波不实现。** 占位，避免把 Octop `tool_guard` 误当成当前开关。

候选：跨 Agent 的可选本机命令提示或拦截（规则文件 + warn/block），挂在各家「允许 / 拒绝 / 一直允许」**之前或旁路**，不替换对方 permission，不默认全员强制 block，不做凭据加密。

未授权前不得改 Settings、不得改确认卡片、不得写入 [STATUS](../STATUS.md)。对照学习项 L-P0-4；产品关闭边见 [产品边界](../decisions/product-boundaries.md)。
