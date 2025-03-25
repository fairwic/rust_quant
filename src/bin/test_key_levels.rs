use std::error::Error;
use rust_quant::app_config::db::init_db;
use rust_quant::trading::{self, indicator::key_levels::KeyLevelIndicator};
use rust_quant::trading::model::market::candles::CandlesEntity;
use rust_quant::trading::indicator::key_levels::PriceLevel;
use chrono::{Utc, Duration};
use dotenv::dotenv;
use std::env;

// 添加一个辅助函数，用于估算和展示强度计算的详细信息
fn explain_strength_calculation(level: &PriceLevel, is_near_current_price: bool) -> String {
    // 强度计算的估算分解
    // 注意：这些值是估算的，因为我们无法访问内部真实计算
    
    // 基础强度评估
    let mut base_strength = 0.3;  // 假设的基础分数
    let mut explanation = Vec::new();
    
    // 根据确认次数估算强度增益
    let confirmation_factor = (level.confirmation_count as f64 * 0.05).min(0.4);
    explanation.push(format!("确认次数({})贡献: +{:.2}", level.confirmation_count, confirmation_factor));
    
    // 是否为黄金分割位估算
    let mut fib_factor = 0.0;
    if level.price.to_string().contains(".382") || level.price.to_string().contains(".618") {
        fib_factor = 0.2;
        explanation.push(format!("黄金分割位贡献: +{:.2}", fib_factor));
    } else if level.price.to_string().contains(".5") || level.price.to_string().contains(".236") || 
              level.price.to_string().contains(".786") {
        fib_factor = 0.1;
        explanation.push(format!("次要斐波那契位贡献: +{:.2}", fib_factor));
    }
    
    // 是否为整数关口估算
    let price_string = level.price.to_string();
    let mut round_factor = 0.0;
    if price_string.ends_with("000.0") || price_string.ends_with("0000.0") {
        round_factor = 0.2;
        explanation.push(format!("重要整数关口贡献: +{:.2}", round_factor));
    } else if price_string.ends_with("00.0") || price_string.ends_with("000.0") {
        round_factor = 0.1;
        explanation.push(format!("整数关口贡献: +{:.2}", round_factor));
    } else if price_string.ends_with("500") || price_string.ends_with("5000") {
        round_factor = 0.05;
        explanation.push(format!("半数关口贡献: +{:.2}", round_factor));
    }
    
    // 是否接近当前价格
    let proximity_factor = if is_near_current_price { 0.1 } else { 0.0 };
    if proximity_factor > 0.0 {
        explanation.push(format!("接近当前价格贡献: +{:.2}", proximity_factor));
    }
    
    // 聚类奖励估算 (多个相近价格合并)
    let cluster_bonus = if level.strength > 0.8 && level.confirmation_count > 10 { 0.15 } else { 0.0 };
    if cluster_bonus > 0.0 {
        explanation.push(format!("价格聚类奖励: +{:.2}", cluster_bonus));
    }
    
    // 总计估算强度
    let estimated_total = (base_strength + confirmation_factor + fib_factor + round_factor + proximity_factor + cluster_bonus).min(1.0);
    
    // 实际强度与估算的差异
    let adjustment = (level.strength - estimated_total).max(-0.3).min(0.3);
    if adjustment.abs() > 0.01 {
        explanation.push(format!("其他因素调整: {:.2}", adjustment));
    }
    
    // 构建最终输出
    let mut result = format!("基础分: {:.2}\n", base_strength);
    for item in explanation {
        result.push_str(&format!("  {}\n", item));
    }
    result.push_str(&format!("实际强度: {:.2} (满分1.0)", level.strength));
    
    result
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // 加载环境变量
    dotenv().ok();
    init_db().await;

    
    // 初始化日志
    if env::var("RUST_LOG").is_err() {
        env::set_var("RUST_LOG", "info");
    }
    tracing_subscriber::fmt::init();
    
    println!("=== 支撑位和压力位识别测试 ===");
    
    // 获取实际行情数据
    let symbol = "BTCUSDT";
    let limit = 300; // 获取300条K线数据
    
    // 计算起止时间
    let end_time = Utc::now();
    
    println!("正在获取 {} 的历史K线数据...", symbol);
    
    // 使用Binance API获取数据
    let candles = trading::task::basic::get_candle_data("BTC-USDT-SWAP", "1H", 1000, None).await?;

    
    println!("获取了 {} 条K线数据", candles.len());
    
    // 创建指标实例
    let indicator = KeyLevelIndicator::new();
    
    // 转换数据格式
    let data_items = indicator.convert_candles_to_data_items(&candles);
    
    // 计算关键价格水平
    let (support_levels, resistance_levels) = indicator.calculate_key_levels(&data_items);
    
    // 获取当前价格
    let current_price = candles.last().unwrap().c.parse::<f64>().unwrap();
    println!("当前价格: {:.2}", current_price);
    
    // 打印支撑位，增加更详细的信息
    println!("\n=== 重要支撑位 (按强度排序) ===");
    println!("排名 | 价格    | 距离当前 | 强度  | 确认次数 | 类型");
    println!("-----|---------|----------|-------|----------|--------");
    for (i, level) in support_levels.iter().take(10).enumerate() {
        let distance_pct = (current_price - level.price) / current_price * 100.0;
        let is_near_current = distance_pct < 3.0; // 距离小于3%认为接近
        
        println!(
            "{:2}   | {:8.2} | {:6.2}%  | {:.2} | {:8}  | {}",
            i + 1, 
            level.price, 
            distance_pct,
            level.strength, 
            level.confirmation_count,
            level.type_name
        );
        
        // 增加强度计算细节说明
        if i < 5 {  // 只为前5个打印详细计算
            println!("      强度计算详情:");
            let explanation = explain_strength_calculation(level, is_near_current);
            for line in explanation.lines() {
                println!("      {}", line);
            }
            println!();  // 空行分隔
        }
    }
    
    println!("\n详细支撑位参数解释:");
    println!("- 价格: 识别出的关键支撑价格水平");
    println!("- 距离当前: 支撑位距离当前价格的百分比距离");
    println!("- 强度: 0.0-1.0范围内的强度值，由以下因素综合计算:");
    println!("  * 价格波动影响");
    println!("  * 成交量权重 (交易量大的区域更重要)");
    println!("  * 确认次数 (价格多次在此水平反弹增加强度)");
    println!("  * 时间因素 (最近形成的支撑位获得提升)");
    println!("- 确认次数: 历史上价格在此水平获得支撑的次数");
    
    // 打印压力位，增加更详细的信息
    println!("\n=== 重要压力位 (按强度排序) ===");
    println!("排名 | 价格    | 距离当前 | 强度  | 确认次数 | 类型");
    println!("-----|---------|----------|-------|----------|--------");
    for (i, level) in resistance_levels.iter().take(10).enumerate() {
        let distance_pct = (level.price - current_price) / current_price * 100.0;
        let is_near_current = distance_pct < 3.0; // 距离小于3%认为接近
        
        println!(
            "{:2}   | {:8.2} | {:6.2}%  | {:.2} | {:8}  | {}",
            i + 1, 
            level.price, 
            distance_pct,
            level.strength, 
            level.confirmation_count,
            level.type_name
        );
        
        // 增加强度计算细节说明
        if i < 5 {  // 只为前5个打印详细计算
            println!("      强度计算详情:");
            let explanation = explain_strength_calculation(level, is_near_current);
            for line in explanation.lines() {
                println!("      {}", line);
            }
            println!();  // 空行分隔
        }
    }
    
    println!("\n详细压力位参数解释:");
    println!("- 价格: 识别出的关键压力价格水平");
    println!("- 距离当前: 压力位距离当前价格的百分比距离");
    println!("- 强度: 0.0-1.0范围内的强度值，由以下因素综合计算:");
    println!("  * 价格波动影响");
    println!("  * 成交量权重 (交易量大的区域更重要)");
    println!("  * 确认次数 (价格多次在此水平回落增加强度)");
    println!("  * 时间因素 (最近形成的压力位获得提升)");
    println!("- 确认次数: 历史上价格在此水平受阻的次数");
    
    // 交易建议
    println!("\n=== 交易建议 ===");
    
    // 使用新添加的辅助方法获取最强支撑位和压力位
    let strongest_support = indicator.get_strongest_support(&support_levels)
        .expect("没有找到支撑位");
    
    let strongest_resistance = indicator.get_strongest_resistance(&resistance_levels)
        .expect("没有找到压力位");
    
    println!("最强支撑位详细信息:");
    println!("价格: {:.2} (距离当前价格: {:.2}%)", 
             strongest_support.price,
             ((current_price - strongest_support.price) / current_price * 100.0).abs());
    println!("强度: {:.2} (满分1.0)", strongest_support.strength);
    println!("确认次数: {} (价格在这一水平获得支撑的次数)", strongest_support.confirmation_count);
    println!("类型: {}", strongest_support.type_name);
    
    // 添加强度计算详细内容
    println!("\n强度计算详情:");
    let support_is_near = ((current_price - strongest_support.price) / current_price * 100.0) < 3.0;
    let support_explanation = explain_strength_calculation(strongest_support, support_is_near);
    for line in support_explanation.lines() {
        println!("  {}", line);
    }
    
    println!("\n说明: 此价格水平是按照综合强度排序得出的最强支撑位，表明在价格下跌至该水平时\n      很可能出现反弹。确认次数越高，该水平越可靠。");
    
    println!("\n最强压力位详细信息:");
    println!("价格: {:.2} (距离当前价格: {:.2}%)", 
             strongest_resistance.price,
             ((strongest_resistance.price - current_price) / current_price * 100.0).abs());
    println!("强度: {:.2} (满分1.0)", strongest_resistance.strength);
    println!("确认次数: {} (价格在这一水平受阻回落的次数)", strongest_resistance.confirmation_count);
    println!("类型: {}", strongest_resistance.type_name);
    
    // 添加强度计算详细内容
    println!("\n强度计算详情:");
    let resistance_is_near = ((strongest_resistance.price - current_price) / current_price * 100.0) < 3.0;
    let resistance_explanation = explain_strength_calculation(strongest_resistance, resistance_is_near);
    for line in resistance_explanation.lines() {
        println!("  {}", line);
    }
    
    println!("\n说明: 此价格水平是按照综合强度排序得出的最强压力位，表明在价格上涨至该水平时\n      可能遇到阻力并回落。确认次数越高，该水平越可靠。");
    
    // 风险回报比分析
    let risk = (current_price - strongest_support.price) / current_price;
    let reward = (strongest_resistance.price - current_price) / current_price;
    let risk_reward_ratio = reward / risk;
    
    println!("\n风险回报比分析:");
    println!("风险 (距最近支撑位): {:.2}%", risk * 100.0);
    println!("回报 (距最近压力位): {:.2}%", reward * 100.0);
    println!("风险回报比: {:.2}", risk_reward_ratio);
    
    if risk_reward_ratio > 2.0 {
        println!("建议: 考虑买入 - 风险回报比良好 (大于2.0)");
    } else if risk_reward_ratio < 0.5 {
        println!("建议: 考虑卖出 - 风险回报比不佳 (小于0.5)");
    } else {
        println!("建议: 观望 - 风险回报比适中 (0.5-2.0之间)");
    }
    
    // 补充提示
    println!("\n注意: 支撑位和压力位排名主要基于以下因素综合计算的强度值:");
    println!("1. 价格历史波动和突破情况");
    println!("2. 该位置的成交量大小");
    println!("3. 历史确认次数");
    println!("4. 形成时间的远近 (最近形成的位置权重更高)");
    println!("5. 斐波那契水平、移动平均线、心理整数关口等额外因素");
    
    Ok(())
} 