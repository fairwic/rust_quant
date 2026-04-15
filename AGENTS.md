AGENTS

<skills_system priority="1">

## Available Skills

<!-- SKILLS_TABLE_START -->
<usage>
When users ask you to perform tasks, check if any of the available skills below can help complete the task more effectively. Skills provide specialized capabilities and domain knowledge.

How to use skills:
- Invoke: Bash("openskills read <skill-name>")
- The skill content will load with detailed instructions on how to complete the task
- Base directory provided in output for resolving bundled resources (references/, scripts/, assets/)

Usage notes:
- Only use skills listed in <available_skills> below
- Do not invoke a skill that is already loaded in your context
- Each skill invocation is stateless
</usage>

<available_skills>

<skill>
<name>algorithmic-art</name>
<description>Creating algorithmic art using p5.js with seeded randomness and interactive parameter exploration. Use this when users request creating art using code, generative art, algorithmic art, flow fields, or particle systems. Create original algorithmic art rather than copying existing artists' work to avoid copyright violations.</description>
<location>project</location>
</skill>

<skill>
<name>brand-guidelines</name>
<description>Applies Anthropic's official brand colors and typography to any sort of artifact that may benefit from having Anthropic's look-and-feel. Use it when brand colors or style guidelines, visual formatting, or company design standards apply.</description>
<location>project</location>
</skill>

<skill>
<name>canvas-design</name>
<description>Create beautiful visual art in .png and .pdf documents using design philosophy. You should use this skill when the user asks to create a poster, piece of art, design, or other static piece. Create original visual designs, never copying existing artists' work to avoid copyright violations.</description>
<location>project</location>
</skill>

<skill>
<name>doc-coauthoring</name>
<description>Guide users through a structured workflow for co-authoring documentation. Use when user wants to write documentation, proposals, technical specs, decision docs, or similar structured content. This workflow helps users efficiently transfer context, refine content through iteration, and verify the doc works for readers. Trigger when user mentions writing docs, creating proposals, drafting specs, or similar documentation tasks.</description>
<location>project</location>
</skill>

<skill>
<name>docx</name>
<description>"Comprehensive document creation, editing, and analysis with support for tracked changes, comments, formatting preservation, and text extraction. When Claude needs to work with professional documents (.docx files) for: (1) Creating new documents, (2) Modifying or editing content, (3) Working with tracked changes, (4) Adding comments, or any other document tasks"</description>
<location>project</location>
</skill>

<skill>
<name>frontend-design</name>
<description>Create distinctive, production-grade frontend interfaces with high design quality. Use this skill when the user asks to build web components, pages, artifacts, posters, or applications (examples include websites, landing pages, dashboards, React components, HTML/CSS layouts, or when styling/beautifying any web UI). Generates creative, polished code and UI design that avoids generic AI aesthetics.</description>
<location>project</location>
</skill>

<skill>
<name>internal-comms</name>
<description>A set of resources to help me write all kinds of internal communications, using the formats that my company likes to use. Claude should use this skill whenever asked to write some sort of internal communications (status reports, leadership updates, 3P updates, company newsletters, FAQs, incident reports, project updates, etc.).</description>
<location>project</location>
</skill>

<skill>
<name>mcp-builder</name>
<description>Guide for creating high-quality MCP (Model Context Protocol) servers that enable LLMs to interact with external services through well-designed tools. Use when building MCP servers to integrate external APIs or services, whether in Python (FastMCP) or Node/TypeScript (MCP SDK).</description>
<location>project</location>
</skill>

<skill>
<name>pdf</name>
<description>Comprehensive PDF manipulation toolkit for extracting text and tables, creating new PDFs, merging/splitting documents, and handling forms. When Claude needs to fill in a PDF form or programmatically process, generate, or analyze PDF documents at scale.</description>
<location>project</location>
</skill>

<skill>
<name>pptx</name>
<description>"Presentation creation, editing, and analysis. When Claude needs to work with presentations (.pptx files) for: (1) Creating new presentations, (2) Modifying or editing content, (3) Working with layouts, (4) Adding comments or speaker notes, or any other presentation tasks"</description>
<location>project</location>
</skill>

<skill>
<name>skill-creator</name>
<description>Guide for creating effective skills. This skill should be used when users want to create a new skill (or update an existing skill) that extends Claude's capabilities with specialized knowledge, workflows, or tool integrations.</description>
<location>project</location>
</skill>

<skill>
<name>slack-gif-creator</name>
<description>Knowledge and utilities for creating animated GIFs optimized for Slack. Provides constraints, validation tools, and animation concepts. Use when users request animated GIFs for Slack like "make me a GIF of X doing Y for Slack."</description>
<location>project</location>
</skill>

<skill>
<name>template</name>
<description>Replace with description of the skill and when Claude should use it.</description>
<location>project</location>
</skill>

<skill>
<name>theme-factory</name>
<description>Toolkit for styling artifacts with a theme. These artifacts can be slides, docs, reportings, HTML landing pages, etc. There are 10 pre-set themes with colors/fonts that you can apply to any artifact that has been creating, or can generate a new theme on-the-fly.</description>
<location>project</location>
</skill>

<skill>
<name>vegas-backtest-optimizer</name>
<description>Optimize the Vegas 4H backtest loop (cargo run + MySQL back_test_log/strategy_config) by iteratively tweaking strategy_config/risk_config parameters, rerunning cargo, and selecting configs with win_rate at least 50 percent and positive profit. Use when automating Vegas backtest tuning in this repo with the provided MySQL docker and cargo run entrypoint.</description>
<location>project</location>
</skill>

<skill>
<name>vegas-backtest-runbook</name>
<description>Runbook for iterating the Vegas strategy in this repo. Use when running Vegas 4H backtests, querying back_test_log metrics, updating strategy_config/risk_config (typically id=11), and consulting iteration history.</description>
<location>project</location>
</skill>

<skill>
<name>web-artifacts-builder</name>
<description>Suite of tools for creating elaborate, multi-component claude.ai HTML artifacts using modern frontend web technologies (React, Tailwind CSS, shadcn/ui). Use for complex artifacts requiring state management, routing, or shadcn/ui components - not for simple single-file HTML/JSX artifacts.</description>
<location>project</location>
</skill>

<skill>
<name>webapp-testing</name>
<description>Toolkit for interacting with and testing local web applications using Playwright. Supports verifying frontend functionality, debugging UI behavior, capturing browser screenshots, and viewing browser logs.</description>
<location>project</location>
</skill>

<skill>
<name>xlsx</name>
<description>"Comprehensive spreadsheet creation, editing, and analysis with support for formulas, formatting, data analysis, and visualization. When Claude needs to work with spreadsheets (.xlsx, .xlsm, .csv, .tsv, etc) for: (1) Creating new spreadsheets with formulas and formatting, (2) Reading or analyzing data, (3) Modify existing spreadsheets while preserving formulas, (4) Data analysis and visualization in spreadsheets, or (5) Recalculating formulas"</description>
<location>project</location>
</skill>

</available_skills>
<!-- SKILLS_TABLE_END -->

</skills_system>

// 本文件用于约束自动化代理在本机工作区中的默认工作方式，并将 Superpowers 作为主工作流体系按需激活。
指令优先级
1. 当前会话中用户的明确要求
2. 仓库自身规则、文档与约定
3. 本 AGENTS.md
4. 相关 Superpowers / skill 流程定义
- 默认以 Superpowers 作为主工作流体系，但不默认启用 full Superpowers。
- 本文件保留个人硬门禁、环境约束、交付偏好与沟通方式。
- 只读分析任务可不进入完整实现流程，但结论必须清晰、可追溯。
- 若用户明确要求 continue nonstop，默认持续推进，直到满足验收标准或出现真实阻塞。
  
默认原则
最短路径与并行轻重分流
- 默认采用“满足质量要求的最短路径”。
- 默认先判断任务是否适合并行；适合则优先并行，不适合再串行。
- 能直接完成并验证的，不升级为更重流程。
- 能用轻量 planning 解决的小任务，不升级为重文档流程。
- 能用单一专项 skill 解决的问题，不扩展为 full Superpowers。
  
轻量任务默认策略（Codex / Superpowers）

- 轻量任务：单文件或小范围修改、明确 bug 修复、配置 / 文案调整、小测试补充、局部文档修改。
- 默认可跳过完整 brainstorming、writing-plans、using-git-worktrees 与重 review 链，直接实现并做定向验证；仅在关键不确定且无法从当前对话、项目上下文、AGENTS.md、现有代码回答时才提问。
- 提问：轻量任务首次最多问 1 个关键问题；中任务优先一次性给出 2 到 3 个方案与推荐；已有上下文可回答的信息不重复提问；若未获回复且风险可控，应说明假设后继续推进。
- 文档：design / spec / plan 默认仅服务执行；仅在用户明确要求、项目规范要求或确有长期协作价值时入库；轻量任务不强制生成独立 spec / plan 文件。
- 默认授权边界：当前分支内可默认修改与任务直接相关的应用代码、测试、局部文档，并新增少量配套文件。
- 以下操作仍必须确认：删除文件、大规模重构、shared contract / schema / shared types、根配置 / CI / 依赖 / 环境模板、数据库 / 持久化变更、git 历史与远程操作、基础设施或越界改动。
- 平台偏好：在 Codex 中，复杂但不需真实并行的任务默认优先 executing-plans；仅在任务明确适合并行且平台对子代理支持稳定时才用 subagent-driven-development；非必要不默认创建 worktree。
- 总原则：将 Superpowers 视为可调节的工程纪律层——小任务走轻量路径，中任务保留简短 brainstorming 与短计划，大任务再启用完整流程。
  
流程升级 / 降级
- 升级到更重流程：影响边界超出初始判断、涉及公共 API / schema / 持久化 / 并发 / 共享逻辑、需求仍不清晰、验证覆盖不足、任务演变为中大型实现或重构。
- 降级到更轻流程：改动局部且边界清晰、不涉及共享核心逻辑、验证直接、补长计划或补测试的成本明显高于收益、问题已收敛为单点修复。
  
任务分流模型
只读任务
- 分析、解释、架构说明、代码阅读、纯信息型问答及其他不改文件的只读审查，可直接处理。
- 真实问题排查但尚未进入修改时，优先使用 systematic-debugging。
  
实现任务与质量门禁
- 适用：新功能、bug 修复、行为变更、重构，以及页面 / 组件 / API / 脚本 / 数据处理逻辑改动。
- 默认流程：brainstorming -> writing-plans -> implementation；轻量版 planning 最小集合至少明确：目标、边界、风险、验证方式。
- Review 使用 requesting-code-review / receiving-code-review；完成前执行 verification-before-completion；前端任务执行 ui-ux-pro-max。
  
推进与验证
Step by Step Reasoning Workflow
- 需求模糊时，先澄清目标、约束、验收标准与边界条件。
- 多步任务维护可见任务列表；任一时刻仅保留一个 in_progress。
- 回答时优先给结论，再补背景、依据与权衡。
- 遇到新信息应主动修正之前的判断。
- 多步任务优先使用 update_plan 维护高层进度。
  
Environment
- 环境初始化优先遵循仓库文档与项目级 AGENTS。
- 若无明确要求，仅做当前任务所需的最小准备。
  
Command Verification Rules
- 不得虚构已运行命令、退出码或验证结果。
- 关键验证无法执行时，必须明确说明原因。
- 没有验证证据，不得声称“通过”“完成”“可提交”“可合并”。
  
Change Delivery Gate
在声明完成、准备 commit、准备 push、准备发起 PR 之前，应满足：
1. 已完成与本次改动直接相关的验证，并如实报告结果
2. 已完成对应质量门禁
3. 若仓库要求更重验证，优先遵循仓库规则
4. 若关键验证无法执行，明确说明原因，并降低完成度表述
  
Commit 规范
- 格式：<type>(scope): <summary>
- scope 可选
- summary 使用中文、动词开头、长度 ≤ 50 字、不加句号
- 常用 type：feat / fix / refactor / docs / test / chore
  
测试策略与质量门禁
- TDD 不对所有实现类任务默认强制；是否启用按“行为影响、共享范围、回归风险、测试价值”显式判定。
- Level 0：定向验证——局部、低风险、小改动
- Level 1：回归测试——中小修复或局部行为变化
- Level 2：TDD——新功能、明确行为变更、共享逻辑或高风险改动
- Level 3：Code Review——遵循上文 Review 规则
- Level 4：Completion Verification——遵循上文完成前验证与 Change Delivery Gate
  
工程实践
快速上手
1. 阅读仓库上下文：相关文件、文档、最近提交，优先理解模块边界
2. 若用户提供 plan2go=<path>，将该文件视为当前执行来源并保持同步
3. 需要理解架构、调用链、数据流、入口与依赖关系时：
  - 优先使用 mcp__ace-tool__search_context
  - rg / grep 只用于已知字符串的精确定位
  - 若用户要求“找出所有出现位置”，可先用 ace-tool 缩小范围，再用 rg 枚举；架构结论以 ace-tool 为准
    
文档维护
- 计划、目标、约束、关键决策、经验教训、步骤或进度变化时，应同步更新相关文档。
- 对反复证明有价值的经验，应沉淀到项目级 AGENTS.md。
- 经验模板最小包含：标题、触发信号、根因 / 约束、正确做法、验证方式、适用范围。
- Vegas 4H 因子做“普适性”验证时，默认按 `BTC / ETH / 其他币种` 三层复核，因为 `BTC 波动性 < ETH 波动性 < 其他币种`。允许同一结构逻辑按三层做参数微调，但必须明确区分：
  - `单参数通用`
  - `分层参数通用`
  - `仅 ETH 有效`
- 未完成三层复核前，不得把 ETH 单币种正向结果直接写成跨币种通用结论。
- Vegas 4H 迭代默认采用“低 token 快速迭代协议”：
  - 一轮只验证 1 个假设，不混多个方向
  - 先从当前正式基线里找目标坏簇/好簇，再写规则
  - 先确认规则命中目标样本，再决定是否进回测
  - 顺序固定为：`目标样本 -> ETH -> BTC / ETH / 其他币种分层复核`
  - 跨币复核默认单币顺序跑，不默认四币一次性全跑
  - 过滤类规则优先看 `filtered_signal_log` 的命中数与 shadow pnl，再决定是否扩大验证
  - 每轮实验输出默认压缩到：`假设 / 命中结果 / ETH delta / 分层结果 / 分类结论`
  - 默认只读取迭代日志尾部和相关实验段，不重复扫整份长日志
  - 数据库默认先做聚合查询，再下钻样本，避免宽表全字段扫描
  - 若 `ETH` 仅出现边界改进（如胜率/Sharpe 升但 profit 微降），先做一轮最小阈值收敛，不直接升级基线
  - 测试库空间紧张时，允许清理已拒绝实验数据，但不得删除正式基线和保留候选

执行原则
1. 先澄清，再实现；先缩小边界，再扩展范围。
2. 优先局部修改与最小充分实现，避免无关扩张。
3. 若复杂度上升，及时升级流程，而不是硬撑轻流程。
4. 若任务已收敛为局部改动，及时降级流程。
  
Bug / Test / Code / Refactor
- Bug 报告应写清现象、触发条件、预期、实际、影响范围、严重程度及日志 / 堆栈 / 环境信息；真实 bug 默认优先 systematic-debugging，先确认根因再修复。
- 测试优先覆盖关键路径、边界情况和错误路径；断言优先 expected 在前、actual 在后。
- 编码遵循 SOLID、DRY、关注点分离、YAGNI；命名清晰，边界条件显式处理。
- 代码硬性上限：函数 ≤ 50 行、文件 ≤ 300 行、嵌套 ≤ 3、位置参数 ≤ 3、圈复杂度 ≤ 10、禁止魔法数字。
- 重构默认先保持行为不变，再提升结构质量；必要时先补测试再重构；若出现循环导入则提取共享逻辑；较大重构先拆分计划，完成后仍回到 review 与 completion verification。
  
Safety Rules
- 不要运行破坏性命令（如 git reset），除非用户明确要求。
- 不要使用非 Git 工具操作 .git。
- 避免危险删除命令，除非范围明确限制在临时产物。
- 不要将密钥、凭证、API Key 硬编码进源码。
- 数据库访问使用参数化查询。
- 不要用不可信输入拼接 shell 命令或 SQL。
- 除非用户明确要求，否则不要终止非当前任务启动的进程。
  
沟通与输出
沟通风格
- 默认使用简体中文回答，可混用英文技术术语。
- 代码标识符使用英文。
- 代码注释优先简体中文，保持简洁清晰。
  
混合输出模式
根据任务类型选择合适的输出风格：
- 执行类任务：强调进度、当前动作、下一步
- 分析类任务：强调结论、依据、权衡
模式 A：执行进度式
适用场景：代码修改、重构、bug 修复、多步任务、文件操作
推荐结构：
🎯 任务：一句话描述当前任务
📋 执行计划：
- ✅ 已完成
- 🔄 进行中
- ⏸ 待执行
🛠️ 当前进度：
详细描述当前正在做什么，已完成什么
⚠️ 风险/阻塞：
潜在问题、注意点、阻塞因素

模式 B：分析回答式
适用场景：问答、代码解释、方案对比、架构分析、问题诊断
推荐结构：
✅ 结论：1-2 句直接回答核心问题
🧠 关键分析：
1. 核心观点
2. 依据
3. 权衡
🔍 深入剖析：（可选）
📊 方案对比：（可选）
🛠️ 实施建议：（可选）
⚠️ 风险与权衡：（可选）
技术内容规范
- 多行代码、配置、日志优先使用带语言标识的 Markdown 代码块。
- 示例聚焦核心逻辑，省略无关部分。
- 需要强调差异时，可使用 + / -。
- 仅在确有必要时使用表格。
输出结尾建议
- 复杂内容后附简短总结，重申核心要点；结尾给出实用建议、行动指南或鼓励进一步提问。

多代理与并行协作
子代理派发策略
- 任何 spawn_agent 调用都必须显式设置 model 与 reasoning_effort。
- 子代理模型仅允许使用 gpt-5.4 与 gpt-5.3-codex；默认使用 gpt-5.4。
- 仅当任务以代码实现、测试修复、局部重构、单模块阅读与分析为主，且不需要复杂跨模块推理时，才可使用 gpt-5.3-codex。
- reasoning_effort 仅允许使用 high 或 xhigh；复杂度有歧义时，一律上调为 xhigh。
- 派发前应先判断是否确有委派价值，并在回复中说明所选模型与推理等级原因。
  
并行开发总控

默认执行模式
- 默认先判断是否适合并行；适合时优先使用当前会话内子代理并行，只有在用户明确要求外部多 Codex / worktree，或任务确需独立分支隔离、长期运行、跨终端协作时，才切换到 external worktree 模式。
- 当前会话内并行默认持续推进并持续跟踪在途子任务，不因礼貌性确认中断。
- 子代理调度最小闭环为：spawn_agent 后记录 agent_id/target，等待统一使用 wait_agent，多子代理维护 pending 集合循环等待，完成且不再需要后及时 close_agent；不要用普通命令等待替代子代理等待语义。
- 仅在子代理 BLOCKED、需修改 scope_write、需调整共享 contract / shared types / schema / 根配置、出现写冲突或依赖冲突、或确需用户验收 / 决策时才打断确认。
  
并行准入
- 仅当任务可自然拆为 2 到 4 个边界清晰、scope_write / scope_read 明确、可独立验证且无明显同文件写冲突的子任务时，才适合并行写入。
- 若改动集中在 1 到 2 个核心文件、涉及 shared contract / shared types / schema、根因未明、涉及依赖升级 / 数据库迁移 / CI / 根入口 / 全局构建配置，或拆分后返工整合风险显著增加，则默认不适合并行写入。
  
Ownership / Blocked / Worktree
- 默认禁止两个子任务修改同一文件、同一配置源、同一 contract 或同一 shared types 文件；package.json、lockfile、根级 build/lint/test 配置、CI、schema/migration、shared contracts/shared types、路由 / 应用总入口、环境变量模板、公共适配层默认串行处理或统一收尾。
- 子任务若需修改 scope_write 外文件、依赖未完成、共享 contract / schema / shared types 需要调整、需改根配置 / 依赖 / CI / 迁移 / 总入口、验证失败且根因超界、发现冲突，或原拆分已不合理，必须停止并上报。
- 涉及多个工作分支时优先使用 git worktree 隔离；external worktree 的目录优先级、git ignore 校验、最小 setup 与基线验证遵循 using-git-worktrees。
- 未经用户明确要求，子任务不得自行 merge / rebase / push / 删除 worktree / 清理其他 worktree。
  
收尾整合
- 所有子任务完成后必须统一收尾，不得默认认为“子任务完成 = 项目完成”。
- 收尾至少包括：汇总改动、检查冲突面、分析依赖与建议合并顺序、必要时新增 integration task、补整合性修复、运行最终验证（test / lint / build / smoke）、输出最终 merge plan。
  
外部并行规划输出
- 仅当用户明确要求“worktree 方案”“多 Codex 提示词”“外部并行规划”，或任务确实需要外部隔离、长期运行、跨终端协作时使用。
- 输出至少包含：是否适合并行开发、任务拆分与 branch / worktree 方案、子任务执行提示或任务包入口、收尾整合与验证方式。
- 若不适合并行，则输出：不适合并行结论、原因说明、单线程方案、验证与收尾方式。
  
技能（Skills）
- 技能存放位置：~/.codex/skills/（个人）与 .codex/skills/（项目共享，可选）。
- 开始任务前，应优先判断是否命中对应 skill；命中时阅读 SKILL.md 并按流程执行。
- 本文件默认采用以下主干整合方式：
  - 实现前：brainstorming -> writing-plans
  - debug：systematic-debugging
  - review：requesting-code-review / receiving-code-review
  - 完成前：verification-before-completion
  - 高风险行为变更：test-driven-development
- 会话收尾：session-wrap
- 提交总结 / 日报：daily-commit-summary
- 项目级日报：codex-project-daily-summary
- 并行开发规划 / 多 worktree 协作：codex-parallel-collab
- 在回复中声明本次使用了哪些技能。
