---
title: AgentHub 文档风格与治理
type: governance
status: current
owner: maintainers
updated: 2026-08-25
---

# 文档风格与治理

本页约束 `docs/` 下的现行 Markdown。目标是让读者先找到正确类型的文档，再得到与源码一致的答案。

## 元数据

现行文档的开头必须有一段机器可读的元数据。新文档优先使用 YAML front matter，至少包含：

```yaml
---
title: 页面标题
type: reference
status: current
owner: maintainers
updated: 2026-08-25
---
```

- `title` 是页面标题，不重复文件名或版本号。
- `type` 必须是以下枚举之一：`navigation`、`tutorial`、`guide`、`how-to`、`getting-started`、`concept`、`reference`、`architecture`、`integration`、`operations`、`ui`、`explanation`、`decision`、`proposal`、`status`、`governance` 或 `archive`。
- `status` 必须是以下枚举之一：`current`、`proposed`、`historical` 或 `archived`。只有 `current` 文档可以作为现行契约引用。
- `owner` 写负责维护该主题的团队或角色；没有专门团队时使用 `maintainers`。它是推荐字段，检查脚本的最低要求是前四项。
- `updated` 使用 `YYYY-MM-DD`，内容或实现事实改变时更新。
- `docs/archive/` 下的历史文件可以没有元数据，但归档索引和新建归档页仍建议补齐。

为兼容已经整理的新目录，短期内也接受紧跟 H1 的引用块元数据：`Status:`、`Type:`、`Last verified:`。其中 `Last verified` 等同于 `updated`，页面 H1 等同于 `title`。兼容块中的 `Architecture` 等同于 `architecture`，`Domain concept` 等同于 `concept`，`Decision index` 与 `Product boundary` 等同于 `decision`；`Current contract`、`Current index` 和 `Current decision` 等同于 `current`。新页面仍应迁移到 YAML，以便工具和站点直接读取。

## 分类

文档只选择一个主要类型，避免一页同时承担互相冲突的任务：

| 类型 | 读者要解决的问题 | 写法 |
| --- | --- | --- |
| `tutorial` | 我第一次如何完成一条完整路径 | 带读者从前置条件走到可见结果 |
| `how-to` | 我已经知道目标，如何完成一次操作 | 步骤短、动作明确、说明验证结果 |
| `reference` | 某个接口、命令、配置或契约是什么 | 结构化、完整、少做解释性散文 |
| `explanation` | 为什么这样设计，边界是什么 | 解释取舍和背景，不伪装成操作步骤 |
| `decision` | 产品或架构决定了什么 | 写结论、理由、范围和替代方案 |
| `proposal` | 候选方向是否值得实施 | 写当前基线、候选目标、门槛和非目标，不伪装成承诺 |
| `status` | 当前实现到哪里，什么还没做 | 只写源码/测试可核实的事实 |
| `governance` | 团队如何维护文档或代码 | 写规则、责任、检查和例外 |
| `navigation` | 应该去哪里继续阅读 | 只放入口、分类和链接 |

这套分类采用 [Diátaxis](https://diataxis.fr/) 的 tutorial、how-to、reference、explanation 四分法，再补充本项目所需的 decision、status、governance 和 navigation。

## 内容与命名

- 文件名使用小写 kebab-case；一个主题只有一个现行真源，其他地方只做短摘要并链接回真源。
- 页面开头先写一句范围说明，再进入章节；标题使用清晰的名词或动作，不用连续感叹号或营销口号。
- UI 文案按实际界面大小写书写；命令、路径、配置键、路由和代码符号使用反引号。
- 操作步骤使用有序列表，每一步以动作开头；先写位置再写动作，保持步骤可独立扫描。
- 表格用于稳定的字段、命令或状态对照；复杂流程使用 Mermaid 或 ASCII 图，并在图后补充文字结论。
- 只引用必要的实现细节。源码路径、函数名和测试名改变时同步更新引用，不为历史文件名制造新的现行契约。

## 状态与维护

- `current` 文档必须能由源码、测试、配置或明确的产品决策验证；不能用旧方案推断当前实现。
- `proposed` 只能描述尚未承诺实施的候选方向；必须写清当前基线和未实施边界，不得出现在当前命令或当前功能列表中。
- `docs/proposals/` 下除 `README.md` 外的提案必须使用 `type: proposal` 与 `status: proposed`；归档目录不受此规则约束。
- `historical` 用于已完成的实施记录、比较或旧方案；`archived` 只用于 `docs/archive/`。
- 完成一次性任务后，删除临时派工稿或移入 `docs/archive/`，并在现行索引保留一个明确的历史入口。
- 每次行为、路径、命令、界面、产品边界或验证矩阵变化，都要在同一个 PR 更新相关文档和 `updated` 日期。
- 新增或修改 Markdown 后运行 `pnpm check:docs`；它会检查本地链接、标题锚点、基础元数据和旧路径术语。

## 链接与历史术语

- 本地链接使用相对路径，链接到文件时包含 `.md`；外部链接使用完整 `https://` URL。
- 链接到标题时确认目标标题实际存在，并避免同一页面产生重复标题锚点。
- 已废弃的用户路径或历史名称必须在同一段或同一行明确标注为旧路径、历史记录或兼容重定向；现行文档正文不得把它写成当前入口。
- 归档文件可以保留历史术语，但归档索引仍应说明其状态。

## 外部方法来源

- [Diátaxis](https://diataxis.fr/)：按读者需求区分 tutorial、how-to、reference 和 explanation。
- [GitLab Documentation Style Guide](https://docs.gitlab.com/development/documentation/styleguide/)：强调文档单一真源、topic type、简洁直接的语气和 UI 操作步骤。
- [GitLab Documentation Folder Structure](https://docs.gitlab.com/development/documentation/site_architecture/folder_structure/)：每个目录以索引页介绍并链接子页面。
- 外部方法只用于组织与写作，不改变 AgentHub 的产品决策、分支规则或安全边界。
