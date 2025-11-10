//! # Rust Quant AI Analysis
//!
//! AI 分析引擎：市场新闻分析、事件热度检测、情绪分析

pub mod event_detector;
pub mod market_impact_predictor;
pub mod news_collector;
pub mod sentiment_analyzer;

// 重新导出核心 Trait
pub use event_detector::EventDetector;
pub use news_collector::NewsCollector;
pub use sentiment_analyzer::SentimentAnalyzer;
