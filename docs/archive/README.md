# 归档文档

这里是**已经落地或被替代**的计划稿、实施记录和带日期审计。不是现行契约，**不要按本文派工**。

现行入口：[../README.md](../README.md)。

| 文件 | 原角色 | 现行去向 |
|---|---|---|
| [a4-unified-loopback-gateway.md](a4-unified-loopback-gateway.md) | A4 统一网关设计稿（曾写 not implemented） | 进程内 Gateway 已落地：`crates/agenthub-core/src/bridge/host/gateway.rs`；契约见 [../provider-api-oauth-adaptation.md](../provider-api-oauth-adaptation.md) §5.4 与 [../local-route-endpoints.md](../local-route-endpoints.md) |
| [routing-connection-refactor-plan.md](routing-connection-refactor-plan.md) | 2026-08-22 四泳道派工 | A1–A4 / C1 等已合入。未完：Claude 订阅→Codex 取证、C2 边默认关。状态见适配规则 §5.4 / §5.5 |
| [multi-account-routing-rfc.md](multi-account-routing-rfc.md) | C2 设计稿 | 内核（AccountPicker）已有；`multi_account` 门默认关。产品开闸仍见 §5.5 |
| [hub-redesign-plan.md](hub-redesign-plan.md) | Hub Phase 1 实施记录 | 现行 IA：[../ui-design.md](../ui-design.md)、[../connection-binding-model.md](../connection-binding-model.md) |
| [route-endpoint-audit-2026-08.md](route-endpoint-audit-2026-08.md) | 2026-08-24 审计快照 | 长期行为：[../local-route-endpoints.md](../local-route-endpoints.md) |
