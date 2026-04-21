use rust_quant_ai_analysis::news_prompt::{build_news_analysis_prompt, NewsAnalysisPromptInput};

#[test]
fn news_analysis_prompt_marks_fast_path_requirements() {
    let prompt = build_news_analysis_prompt(&NewsAnalysisPromptInput {
        title: "SEC 批准现货比特币 ETF".to_string(),
        content: "美国 SEC 已批准多只现货比特币 ETF 上市交易。".to_string(),
        source: "jinse".to_string(),
        published_at: "2026-04-21T10:00:00Z".to_string(),
    });

    assert!(prompt.system.contains("高置信历史模式快速通道"));
    assert!(prompt.system.contains("fast_path"));
    assert!(prompt.system.contains("historical_pattern"));
    assert!(prompt.system.contains("impact_score"));
    assert!(prompt.system.contains("confidence"));
    assert!(prompt.system.contains("ETF"));
    assert!(prompt.system.contains("只输出 JSON"));

    assert!(prompt.user.contains("SEC 批准现货比特币 ETF"));
    assert!(prompt
        .user
        .contains("美国 SEC 已批准多只现货比特币 ETF 上市交易。"));
    assert!(prompt.user.contains("jinse"));
}

#[test]
fn news_analysis_prompt_details_fast_path_event_rules() {
    let prompt = build_news_analysis_prompt(&NewsAnalysisPromptInput {
        title: "测试标题".to_string(),
        content: "测试内容".to_string(),
        source: "test".to_string(),
        published_at: "2026-04-21T10:00:00Z".to_string(),
    });

    let system = prompt.system;
    for required in [
        "触发词",
        "方向判断",
        "降级 full_analysis",
        "现货 ETF 获批",
        "监管禁令",
        "CPI/PCE/非农",
        "暂停提现",
        "稳定币脱锚",
        "跨链桥被盗",
        "大额解锁",
        "主网上线延期",
        "conflicting_info",
    ] {
        assert!(system.contains(required), "missing prompt rule: {required}");
    }
}
