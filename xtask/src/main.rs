//! 架构防腐检查器(只读)。
//!
//! 用法:`cargo xtask arch-check [--json]`
//!
//! 把 dependency-rules.md §13 中当前可静态检查的规则落地为检查器,并按
//! `docs/architecture/migrations/baseline-2026-07/legacy-allowlist.toml` 冻结的
//! legacy 违规基线做 ratchet:运行违规数 > 基线即 FAIL,<= 基线通过。
//!
//! 本工具不修改任何源码,不做自动修复,不进生产依赖图(见 xtask/Cargo.toml)。

mod baseline;
mod checks;
mod model;
mod report;

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cmd = args.first().map(String::as_str).unwrap_or("arch-check");
    let json = args.iter().any(|a| a == "--json");

    match cmd {
        "arch-check" => match checks::run_arch_check() {
            Ok(outcome) => {
                report::print(&outcome, json);
                if outcome.failed() {
                    ExitCode::FAILURE
                } else {
                    ExitCode::SUCCESS
                }
            }
            Err(e) => {
                eprintln!("xtask arch-check 内部错误: {e}");
                ExitCode::FAILURE
            }
        },
        other => {
            eprintln!("未知子命令: {other}。可用: arch-check [--json]");
            ExitCode::FAILURE
        }
    }
}
