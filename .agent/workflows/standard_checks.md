---
description: Run standard Rust verification checks (format, clippy, test) for the rust_quant workspace
---

# universal_ai_thought_workflow.yaml
name: "universal_ai_thought_workflow"
description: |
  通用 AI 思维驱动编码 Workflow。
  目标：在任何语言/框架仓库中，让 AI 像资深工程师一样“先思考再改代码”，
  最大限度降低 hallucination、破坏性改动与无谓 diff。
# 全局约束
globals:
  ignore_paths:
    - "node_modules"
    - "venv"
    - "dist"
    - "build"
    - ".git"
    - ".venv"
    - "logs"
    - "target"
  forbidden_paths:
    - "prod_secrets/**"
    - "infrastructure/terraform/state/**"
  max_lines_per_file_diff: 200
  max_total_changed_files_without_confirm: 6
  require_user_confirm_for_high_risk: true
  high_risk_threshold:
    changed_files: 6
    total_lines_changed: 500

steps:
  - id: clarify_intent
    name: "Clarify Problem (可执行意图确认)"
    type: "prompt"
    instruction: |
      目的：把用户的自然语言目标转换为“可执行意图（Executable Intent）”，并识别不明确项。
      输出 JSON 必须包含以下字段（严格格式）：
      {
        "raw_user_request": "<原始文本>",
        "executable_intent": "<一句话描述可执行目标>",
        "task_type": "create | modify | refactor | fix | delete | investigate",
        "success_criteria": ["<怎样判断完成>"],
        "ambiguities": ["<需要用户澄清的问题，如目标路由/函数名/边界条件>"],
        "requires_repo_context": true|false
      }
      规则：
      - 如果 ambiguities 非空，停止后续步骤并把 ambiguities 返回给用户（无需继续扫描仓库）。
      - 不要假设未给出的信息，必须列出所有疑问。

  - id: repo_context
    name: "Repo Deep Context (深度上下文扫描)"
    type: "script"
    run: "scan-repo --ignore ${globals.ignore_paths}"
    instruction: |
      目的：构建精确的项目 mental map（语言、模块、入口、重要文件、依赖、测试和编译命令）。
      输出 JSON（structured）：
      {
        "languages": ["rust","go","typescript",...],
        "modules": [
          {"path":"service/a","role":"service","entry_points":["main.rs"], "public_apis":["/api/v1/..."], "dependencies":["crate_x"]},
          ...
        ],
        "build": {"commands":["cargo build","mvn -q package","python -m build"], "type_check":"cargo check | tsc --noEmit | mypy"},
        "test": {"commands":["cargo test","go test ./...","pytest -q"], "has_tests": true|false},
        "linters": ["eslint","clippy","golangci-lint","ruff"],
        "code_style_files": ["pyproject.toml",".prettierrc",".clang-format"],
        "important_files": ["README.md","package.json","Cargo.toml","pom.xml","Dockerfile"],
        "risks": ["circular dependency: ...", "no type checker", ...]
      }
      规则：
      - 所有输出字段必须基于实际扫描结果（不能猜测）。
      - 如果仓库过大，只扫描与 executable_intent 相关的子树（见下步 context_map）。

  - id: context_map
    name: "Construct Minimal Context Map (最小上下文集)"
    type: "compute"
    instruction: |
      目的：基于 repo_context 与 executable_intent，生成“最小上下文集”，仅包含与任务直接相关的文件/模块。
      输出：
      {
        "relevant_files": ["path/a","path/b"],
        "relevant_modules": [...],
        "entry_points": [...],
        "external_apis_called": [...],
        "dependency_graph_subset": {...}
      }
      规则：
      - 限制上下文规模，避免扫描大型无关目录。
      - 列出任何“跨语言/跨模块一致性风险”。

  - id: interpret
    name: "Interpret -> Structured Task (把需求拆成指令级任务)"
    type: "prompt"
    inputs: ["clarify_intent.output","context_map.output"]
    instruction: |
      目的：将执行意图转为结构化任务树（Task Graph）。输出 JSON：
      {
        "task_graph": [
          {
            "id": "t1",
            "action": "modify_file | create_file | update_dependency | add_test | refactor",
            "targets": ["relevant/file/path"],
            "goal": "一句话目的",
            "preconditions": ["file exists","build passes"],
            "estimated_impact": {"files_changed":n,"approx_lines":m},
            "allowed_changes": "minimal | moderate | major"
          }, ...
        ],
        "overall_risk": "low|medium|high",
        "missing_info": ["..."]  // 如需更多参数
      }
      规则：
      - 每个 task 的 targets 必须是在 context_map.relevant_files 里，或明确标注新建文件路径。
      - 如果 any task estimated_impact 超过 globals.high_risk_threshold，标记为 high risk 并要求用户确认（如果 require_user_confirm_for_high_risk=true）。

  - id: plan
    name: "Plan (可执行计划 & 备选方案)"
    type: "compute"
    inputs: ["interpret.output"]
    instruction: |
      目的：把 task_graph 转为严格执行步骤序列，每步包含 success_criteria & fallback。
      输出示例：
      {
        "steps": [
          {
            "id":"step-1",
            "description":"在文件 X 中新增 validate 函数",
            "action":"modify_file",
            "file":"path/to/X",
            "preconditions":["file contains function foo()"],
            "success_criteria":["lint ok","build ok","tests covering validate pass"],
            "fallback":"revert changes and report"
          }, ...
        ],
        "chosen_solution_reasoning": "为什么选择该实施路径",
        "alternatives": ["如果选择B会怎样，成本与风险"]
      }
      规则：
      - 每个步骤必须短小（单一职责），并可单独验证。

  - id: precheck
    name: "Pre-Code Existence & Safety Check (存在性与安全检查)"
    type: "script"
    inputs: ["plan.output","context_map.output"]
    run: "precheck-script --plan plan.json --context context.json"
    instruction: |
      检查点：
      - targets 文件/函数确实存在（除非明确 new file）
      - forbidden_paths 未被触及
      - 不会修改超过 globals.max_total_changed_files_without_confirm
      输出：
      {
        "ok": true|false,
        "violations": [...],
        "why_blocked": "..."
      }
      规则：
      - 若 ok=false，则停止并把 violations 返回用户。

  - id: code_sketch
    name: "Code Sketch (草图合成)"
    type: "prompt"
    inputs: ["plan.output","context_map.output"]
    instruction: |
      目的：先产出“草图实现（Code Sketch）”——伪代码或最小可运行示例（不直接提交 diff）。
      输出：
      {
        "for_each_task": [
          {
            "task_id":"t1",
            "sketch":"关键函数/接口/数据结构伪代码",
            "interfaces_needed":[ "function signatures", "types" ],
            "points_of_attention":[ "边界条件", "性能" ]
          }, ...
        ]
      }
      规则：
      - 草图要明确函数签名/类型/异常契约/返回结构。
      - 草图不得包含 platform-only commands (比如替换全局配置)，只描述代码层面改变。

  - id: synthesize
    name: "Synthesize Implementation (从草图到精炼实现)"
    type: "prompt"
    inputs: ["code_sketch.output","context_map.output"]
    instruction: |
      目的：基于 code_sketch 生成满足项目风格的实现（但仍以 diff 草案形式返回，不直接写文件）。
      输出必须包含：
      {
        "diffs": [
          { "file":"a/b.cpp", "patch":"@@ -1,4 +1,6 @@\n+ added line\n" },
          ...
        ],
        "rationale_per_diff": ["简短原因"],
        "imports_or_deps_added": ["crate_x","library_y"],
        "estimated_lines_changed_per_file": { "a/b.cpp": 12 }
      }
      规则：
      - 不得改动 context_map 未列出的文件。
      - 若需要新增依赖，必须在 imports_or_deps_added 明确声明并在下一步更新依赖文件。

  - id: codegen
    name: "Generate Strict Diff (严格 diff 输出)"
    type: "compute"
    inputs: ["synthesize.output"]
    instruction: |
      目的：对 synthesize 的草案执行一轮规则校验（风格/行数/禁止变更），并最终输出“可打补丁的 diff”。
      验证点：
      - 每个文件 diff 行数 <= globals.max_lines_per_file_diff
      - 总 changed files <= globals.max_total_changed_files_without_confirm or user confirmed
      - 未触及 forbidden_paths
      输出：
      {
        "final_diffs": [...],
        "meta": {"total_files_changed":n,"total_lines_changed":m},
        "requires_user_confirmation": true|false,
        "reasons": [...]
      }
      规则：
      - 如果 requires_user_confirmation true，则停止并把 summary 返回用户。

  - id: verify
    name: "Verify (lint/typecheck/build/test)"
    type: "script"
    inputs: ["codegen.final_diffs","repo_context.output"]
    run: |
      # 脚本逻辑（伪示例）
      apply_patch --diff codegen.final_diffs --dry-run
      run_linters_based_on repo_context.linters
      run_type_checkers_based_on repo_context.build.type_check
      run_build_command
      run_tests_if_present
    instruction: |
      目的：在 sandbox（或 dry-run）中执行验证。输出：
      {
        "lint_ok": true|false,
        "type_check_ok": true|false,
        "build_ok": true|false,
        "tests_ok": true|false,
        "failures": [
          {"stage":"build","error":"...","suggested_fix":"..."}
        ]
      }
      规则：
      - 若有失败，进入 auto_fix 步骤（最多两轮自动修复），否则继续。

  - id: auto_fix
    name: "Auto-Fix (自动修复失败)"
    type: "prompt+script"
    inputs: ["verify.output","synthesize.output"]
    instruction: |
      目的：尝试在保证 minimal change 的前提下自动修复 lint/build/test 报错，输出修复 diff 并再次验证。
      规则：
      - 自动修复不得扩大原计划的 allowed_changes 级别（例如从 minimal 变为 major）。
      - 自动修复失败或需要超范围改动时，停止并把详细错误/建议返回用户。

  - id: self_review
    name: "Self Review (工程师式自审)"
    type: "prompt"
    inputs: ["codegen.final_diffs","verify.output","plan.output"]
    instruction: |
      目的：AI 以资深工程师身份做一次自审（可输出 checklist），必须回答：
      - 变更是否修改了公共 API？若修改，是否有迁移策略？
      - 是否引入安全/并发/性能隐患？
      - 是否需要新增测试，具体哪些用例？
      - 是否有剩余技术债？如何偿还？
      输出示例：
      {
        "public_api_changes": [...],
        "risks_identified":[...],
        "recommended_tests":[...],
        "tech_debt":[...],
        "acceptance": "ready_to_apply | needs_user_review"
      }

  - id: apply_patch
    name: "Apply Patch / Commit"
    type: "script"
    inputs: ["codegen.final_diffs","self_review.output"]
    run: |
      if [ self_review.output.acceptance == "ready_to_apply" ]; then
        apply_patch --diff codegen.final_diffs
        git commit -m "AI: <short description> (workflow: universal_ai_thought_workflow)"
      else
        echo "needs_user_review"
      fi
    instruction: |
      规则：
      - 仅当 self_review.acceptance 为 ready_to_apply 且 verify 全部通过才自动提交。
      - 否则生成 pull request 草案并标注需要人工复核。

  - id: summary
    name: "Final Summary (审计级报告)"
    type: "prompt"
    inputs: ["apply_patch.output","self_review.output","verify.output","plan.output"]
    instruction: |
      输出完整审计级 JSON 报告，包含：
      {
        "user_request": "<原始>",
        "executable_intent": "...",
        "task_graph": [...],
        "final_changes": {"files_changed": [...], "new_files": [...], "patch_urls": "..."},
        "verification": {"lint_ok":...,"build_ok":...,"tests_ok":...},
        "self_review": {...},
        "risks": [...],
        "next_steps_for_user": ["如何手动验证","回滚方法","监控点"]
      }
      并生成一段简短的自然语言汇报，方便贴入 PR 描述或 Slack。

# 可扩展点（可选）
# - multi_agent: 可以把 code_sketch / synthesize / self_review 分配给不同 agent（架构 AI / 代码 AI / 审查 AI）
# - telemetry: 记录每次修改的 metric（lines changed, time spent, failures）
# - plugins: CI runner plugins 用于 sandbox 验证（容器化执行）

# 使用说明（简短）
# 1. 将此 YAML 复制到你的 Cursor / Antigravity 工作流编辑器里（根据平台字段名微调 step type/run 字段）。
# 2. 平台需要支持：运行自定义脚本、在 sandbox/dry-run 中应用 patch 并运行构建/测试。
# 3. 在首次运行时，注意设置 require_user_confirm_for_high_risk，根据团队容忍度调整阈值。

