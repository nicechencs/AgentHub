# 测试样例（非生产代码）

本目录**仅**供 vitest 回归使用，不会被 `@/lib/provider-detect` 生产入口导出。

- 内容为用户提供的**形态示例**（export / set / $env / settings.json / config.toml 等）
- 其中的 URL、Key 是**占位/示例值**，生产识别按正则抽取用户真实粘贴内容
- 新增样式：加常量 → 挂 `*_SAMPLES` → `fixtures.test.ts` 自动跑
