//! arch-check 结果输出:人读摘要 + 可选 JSON。

use crate::model::Outcome;

pub fn print(outcome: &Outcome, json: bool) {
    if json {
        match serde_json::to_string_pretty(outcome) {
            Ok(s) => println!("{s}"),
            Err(e) => eprintln!("序列化 JSON 失败: {e}"),
        }
        return;
    }

    println!("== cargo xtask arch-check ==");
    for c in &outcome.checks {
        let total = c.violations.len();
        let baseline_hits = c
            .violations
            .iter()
            .filter(|v| v.baseline_id.is_some())
            .count();
        let new_hits = total - baseline_hits;
        println!(
            "[{}] 违规 {total}(基线内 {baseline_hits} / 新增 {new_hits}),告警 {}",
            c.name,
            c.warnings.len()
        );
        for w in &c.warnings {
            println!("    WARN  {w}");
        }
    }

    if !outcome.resolved_baseline_ids.is_empty() {
        println!(
            "\n已消除的基线违规(违规数下降): {}",
            outcome.resolved_baseline_ids.join(", ")
        );
    }

    if !outcome.new_violations.is_empty() {
        println!("\n新增违规(禁止,导致 FAIL):");
        for v in &outcome.new_violations {
            println!("    FAIL  [{}] {} @ {}", v.category, v.detail, v.location);
        }
    }

    if !outcome.hard_size_failures.is_empty() {
        println!("\n文件行数硬失败(>2000 行):");
        for f in &outcome.hard_size_failures {
            println!("    FAIL  {f}");
        }
    }

    println!();
    if outcome.failed() {
        println!("结果: FAIL");
    } else {
        println!("结果: PASS(违规数未超过冻结基线)");
    }
}
