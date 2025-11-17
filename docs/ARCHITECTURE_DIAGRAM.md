# Rust Quant 系统架构图

> **⚠️ 架构重构中**  
> 当前架构存在职责不清、依赖混乱的问题，正在重构中。  
> 详见：[架构重构方案](./ARCHITECTURE_REDESIGN.md)

## 0. 理想架构（目标）

```mermaid
graph TB
    subgraph "Layer 1: Application Layer（应用层）"
        CLI[rust-quant-cli<br/>程序入口]
        ORCH[orchestration<br/>任务编排、调度]
    end

    subgraph "Layer 2: Domain Layer（领域层）"
        MARKET[market<br/>市场数据领域<br/>数据采集、存储、查询]
        TRADING[trading<br/>交易领域<br/>策略、执行、风控]
    end

    subgraph "Layer 3: Domain Layer（领域层）"
        DOMAIN[domain<br/>核心领域模型<br/>实体、值对象、接口]
    end

    subgraph "Layer 4: Infrastructure Layer（基础设施层）"
        INFRA[infrastructure<br/>数据访问<br/>Repository实现]
        INDICATORS[indicators<br/>技术指标库<br/>纯计算工具]
        EXCHANGES[exchanges<br/>交易所适配器<br/>外部服务封装]
    end

    subgraph "Layer 4: Core Layer（核心基础设施）"
        CORE[core<br/>配置、数据库、缓存、日志]
        COMMON[common<br/>通用工具]
    end

    subgraph "Layer 5: Analysis Layer（分析层 - 可选）"
        ANALYTICS[analytics<br/>性能分析]
        AI[ai-analysis<br/>AI分析]
    end

    CLI --> ORCH
    ORCH --> MARKET
    ORCH --> TRADING

    MARKET --> DOMAIN
    TRADING --> DOMAIN

    INFRA --> DOMAIN
    EXCHANGES --> DOMAIN
    INDICATORS --> COMMON

    MARKET --> INFRA
    TRADING --> INFRA
    TRADING --> EXCHANGES
    TRADING --> INDICATORS

    INFRA --> CORE
    EXCHANGES --> CORE
    MARKET --> CORE
    TRADING --> CORE

    ANALYTICS --> INFRA
    AI --> MARKET

    style CLI fill:#ff6b6b
    style ORCH fill:#4ecdc4
    style DOMAIN fill:#f38181
    style MARKET fill:#95e1d3
    style TRADING fill:#fcbad3
    style INFRA fill:#aa96da
    style INDICATORS fill:#a8d8ea
    style CORE fill:#dcedc1
    style COMMON fill:#ffeaa7
```

**关键改进**：
1. **领域分离**：`market`（市场数据）和 `trading`（交易）是两个独立领域
2. **技术指标降级**：`indicators` 从业务层降级为基础设施层（纯计算工具）
3. **交易领域整合**：`strategies`、`risk`、`execution` 合并到 `trading` 领域
4. **依赖清晰**：Application → Domain → Infrastructure → Core

---

## 1. 当前架构（待重构）

```mermaid
graph TB
    subgraph "应用入口层"
        CLI[rust-quant-cli<br/>命令行入口]
    end

    subgraph "编排层 Orchestration"
        SCHEDULER[调度器<br/>Scheduler]
        STRATEGY_RUNNER[策略运行器<br/>Strategy Runner]
        BACKTEST[回测引擎<br/>Backtest]
        JOBS[定时任务<br/>Jobs]
        WORKFLOW[工作流<br/>Workflow]
    end

    subgraph "服务层 Services"
        STRATEGY_SVC[策略服务<br/>Strategy Service]
        MARKET_SVC[市场服务<br/>Market Service]
        RISK_SVC[风控服务<br/>Risk Service]
        EXCHANGE_SVC[交易所服务<br/>Exchange Service]
        TRADING_SVC[交易服务<br/>Trading Service]
    end

    subgraph "领域层 Domain"
        ENTITIES[实体<br/>Entities]
        VALUE_OBJECTS[值对象<br/>Value Objects]
        ENUMS[枚举<br/>Enums]
        TRAITS[接口<br/>Traits]
    end

    subgraph "业务逻辑层"
        STRATEGIES[策略引擎<br/>Strategies]
        INDICATORS[技术指标<br/>Indicators]
        RISK_LOGIC[风控逻辑<br/>Risk]
        EXECUTION[订单执行<br/>Execution]
    end

    subgraph "基础设施层 Infrastructure"
        REPOS[数据仓储<br/>Repositories]
        CACHE[缓存<br/>Cache]
        EXCHANGES[交易所适配器<br/>Exchanges]
        MESSAGING[消息传递<br/>Messaging]
    end

    subgraph "核心基础设施 Core"
        CONFIG[配置管理<br/>Config]
        DB[数据库连接池<br/>Database Pool]
        REDIS[Redis客户端<br/>Redis Client]
        LOGGER[日志系统<br/>Logger]
    end

    subgraph "通用工具 Common"
        UTILS[工具函数<br/>Utils]
        TYPES[通用类型<br/>Types]
        ERRORS[错误定义<br/>Errors]
        CONSTANTS[常量<br/>Constants]
    end

    subgraph "市场数据 Market"
        WEBSOCKET[WebSocket流<br/>WebSocket Stream]
        CANDLE_REPO[K线数据<br/>Candle Repository]
        TICKER_REPO[行情数据<br/>Ticker Repository]
    end

    subgraph "分析层 Analytics"
        PERFORMANCE[性能分析<br/>Performance]
        REPORTING[报表生成<br/>Reporting]
    end

    subgraph "AI分析层 AI-Analysis"
        SENTIMENT[情绪分析<br/>Sentiment Analyzer]
        EVENT[事件检测<br/>Event Detector]
        IMPACT[市场影响预测<br/>Market Impact Predictor]
    end

    CLI --> SCHEDULER
    CLI --> STRATEGY_RUNNER
    CLI --> BACKTEST

    SCHEDULER --> JOBS
    SCHEDULER --> WORKFLOW
    STRATEGY_RUNNER --> STRATEGY_SVC
    BACKTEST --> STRATEGY_SVC

    STRATEGY_SVC --> STRATEGIES
    STRATEGY_SVC --> INDICATORS
    MARKET_SVC --> WEBSOCKET
    MARKET_SVC --> CANDLE_REPO
    RISK_SVC --> RISK_LOGIC
    EXCHANGE_SVC --> EXCHANGES
    TRADING_SVC --> EXECUTION

    STRATEGIES --> TRAITS
    INDICATORS --> TRAITS
    RISK_LOGIC --> TRAITS
    EXECUTION --> TRAITS

    STRATEGIES --> INDICATORS
    EXECUTION --> RISK_LOGIC

    REPOS --> TRAITS
    CACHE --> REDIS
    EXCHANGES --> TRAITS

    STRATEGY_SVC --> REPOS
    MARKET_SVC --> REPOS
    RISK_SVC --> REPOS
    EXCHANGE_SVC --> REPOS

    REPOS --> DB
    CACHE --> REDIS
    CONFIG --> DB
    CONFIG --> REDIS
    CONFIG --> LOGGER

    STRATEGIES --> COMMON
    INDICATORS --> COMMON
    SERVICES --> COMMON
    INFRASTRUCTURE --> COMMON

    PERFORMANCE --> REPOS
    REPORTING --> REPOS
    SENTIMENT --> MARKET_SVC
    EVENT --> MARKET_SVC
    IMPACT --> MARKET_SVC

    style CLI fill:#e1f5ff
    style SCHEDULER fill:#fff4e1
    style STRATEGY_SVC fill:#e8f5e9
    style ENTITIES fill:#f3e5f5
    style STRATEGIES fill:#fff9c4
    style REPOS fill:#fce4ec
    style CONFIG fill:#e0f2f1
```

## 2. 模块依赖关系图

```mermaid
graph LR
    subgraph "依赖方向: 从上到下"
        CLI[rust-quant-cli]
        ORCH[orchestration]
        SVC[services]
        DOMAIN[domain]
        INFRA[infrastructure]
        STRATEGIES[strategies]
        INDICATORS[indicators]
        RISK[risk]
        EXECUTION[execution]
        MARKET[market]
        CORE[core]
        COMMON[common]
        ANALYTICS[analytics]
        AI[ai-analysis]
    end

    CLI --> ORCH
    CLI --> CORE
    CLI --> COMMON

    ORCH --> SVC
    ORCH --> DOMAIN
    ORCH --> INFRA
    ORCH --> CORE
    ORCH --> COMMON

    SVC --> DOMAIN
    SVC --> INFRA
    SVC --> STRATEGIES
    SVC --> INDICATORS
    SVC --> RISK
    SVC --> EXECUTION
    SVC --> MARKET
    SVC --> CORE
    SVC --> COMMON

    STRATEGIES --> DOMAIN
    STRATEGIES --> INDICATORS
    STRATEGIES --> COMMON

    INDICATORS --> DOMAIN
    INDICATORS --> COMMON

    RISK --> DOMAIN
    RISK --> COMMON

    EXECUTION --> DOMAIN
    EXECUTION --> RISK
    EXECUTION --> COMMON

    MARKET --> DOMAIN
    MARKET --> CORE
    MARKET --> COMMON

    INFRA --> DOMAIN
    INFRA --> CORE
    INFRA --> COMMON

    ANALYTICS --> DOMAIN
    ANALYTICS --> INFRA
    ANALYTICS --> COMMON

    AI --> DOMAIN
    AI --> MARKET
    AI --> COMMON

    style CLI fill:#ff6b6b
    style ORCH fill:#4ecdc4
    style SVC fill:#95e1d3
    style DOMAIN fill:#f38181
    style INFRA fill:#aa96da
    style STRATEGIES fill:#fcbad3
    style INDICATORS fill:#a8d8ea
    style RISK fill:#ffd3a5
    style EXECUTION fill:#fd9853
    style MARKET fill:#a8e6cf
    style CORE fill:#dcedc1
    style COMMON fill:#ffeaa7
```

## 3. 策略执行数据流

```mermaid
sequenceDiagram
    participant CLI as CLI入口
    participant SCHEDULER as 调度器
    participant STRATEGY_SVC as 策略服务
    participant INDICATORS as 技术指标
    participant STRATEGIES as 策略引擎
    participant RISK_SVC as 风控服务
    participant EXCHANGE_SVC as 交易所服务
    participant OKX_SVC as OKX订单服务
    participant REPO as 数据仓储
    participant REDIS as Redis缓存
    participant DB as MySQL数据库

    CLI->>SCHEDULER: 启动策略任务
    SCHEDULER->>STRATEGY_SVC: 执行策略
    
    STRATEGY_SVC->>REPO: 获取K线数据
    REPO->>DB: 查询历史K线
    DB-->>REPO: 返回K线数据
    REPO-->>STRATEGY_SVC: K线数据
    
    STRATEGY_SVC->>INDICATORS: 计算技术指标
    INDICATORS-->>STRATEGY_SVC: 指标值
    
    STRATEGY_SVC->>STRATEGIES: 生成交易信号
    STRATEGIES-->>STRATEGY_SVC: SignalResult
    
    alt 有交易信号
        STRATEGY_SVC->>RISK_SVC: 风控检查
        RISK_SVC-->>STRATEGY_SVC: 风控通过
        
        STRATEGY_SVC->>EXCHANGE_SVC: 获取API配置
        EXCHANGE_SVC->>REDIS: 查询缓存
        alt 缓存命中
            REDIS-->>EXCHANGE_SVC: API配置
        else 缓存未命中
            EXCHANGE_SVC->>REPO: 查询数据库
            REPO->>DB: 查询API配置
            DB-->>REPO: API配置
            REPO-->>EXCHANGE_SVC: API配置
            EXCHANGE_SVC->>REDIS: 更新缓存
        end
        
        EXCHANGE_SVC-->>STRATEGY_SVC: API配置
        
        STRATEGY_SVC->>OKX_SVC: 执行下单
        OKX_SVC->>OKX_SVC: 创建OKX客户端
        OKX_SVC->>OKX_API: 调用交易所API
        OKX_API-->>OKX_SVC: 订单结果
        OKX_SVC-->>STRATEGY_SVC: 订单结果
        
        STRATEGY_SVC->>REPO: 保存订单记录
        REPO->>DB: 插入订单
    end
    
    STRATEGY_SVC-->>SCHEDULER: 执行完成
    SCHEDULER-->>CLI: 任务完成
```

## 4. 回测系统架构

```mermaid
graph TB
    subgraph "回测入口"
        BACKTEST_RUNNER[BacktestRunner<br/>回测运行器]
    end

    subgraph "回测执行"
        BACKTEST_EXECUTOR[BacktestExecutor<br/>回测执行器]
        PARAM_GENERATOR[ParamGenerator<br/>参数生成器]
        PROGRESS_MGR[ProgressManager<br/>进度管理器]
    end

    subgraph "策略层"
        VEGAS_STRATEGY[Vegas策略<br/>VegasStrategy]
        NWE_STRATEGY[NWE策略<br/>NweStrategy]
        BACKTEST_GENERIC[通用回测<br/>run_back_test_generic]
    end

    subgraph "指标层"
        INDICATORS_CALC[指标计算<br/>Indicators]
    end

    subgraph "数据层"
        CANDLE_REPO[K线仓储<br/>CandleRepository]
        SIGNAL_LOG[信号日志<br/>SignalLogRepository]
        BACKTEST_REPO[回测结果<br/>BacktestRepository]
    end

    subgraph "存储"
        REDIS[Redis<br/>进度缓存]
        MYSQL[MySQL<br/>结果存储]
    end

    BACKTEST_RUNNER --> BACKTEST_EXECUTOR
    BACKTEST_RUNNER --> PARAM_GENERATOR
    BACKTEST_RUNNER --> PROGRESS_MGR

    BACKTEST_EXECUTOR --> VEGAS_STRATEGY
    BACKTEST_EXECUTOR --> NWE_STRATEGY
    BACKTEST_EXECUTOR --> BACKTEST_GENERIC

    VEGAS_STRATEGY --> INDICATORS_CALC
    NWE_STRATEGY --> INDICATORS_CALC
    BACKTEST_GENERIC --> INDICATORS_CALC

    BACKTEST_EXECUTOR --> CANDLE_REPO
    BACKTEST_EXECUTOR --> SIGNAL_LOG
    BACKTEST_EXECUTOR --> BACKTEST_REPO

    PROGRESS_MGR --> REDIS
    CANDLE_REPO --> MYSQL
    SIGNAL_LOG --> MYSQL
    BACKTEST_REPO --> MYSQL

    style BACKTEST_RUNNER fill:#e1f5ff
    style BACKTEST_EXECUTOR fill:#fff4e1
    style VEGAS_STRATEGY fill:#e8f5e9
    style INDICATORS_CALC fill:#fff9c4
    style REDIS fill:#fce4ec
    style MYSQL fill:#e0f2f1
```

## 5. 市场数据流架构

```mermaid
graph LR
    subgraph "交易所"
        OKX_API[OKX API]
        BINANCE_API[Binance API<br/>未来扩展]
    end

    subgraph "数据采集层"
        WEBSOCKET[WebSocket服务<br/>实时数据流]
        REST_API[REST API<br/>历史数据]
    end

    subgraph "数据处理层"
        CANDLE_SVC[Candle服务<br/>K线处理]
        TICKER_SVC[Ticker服务<br/>行情处理]
        PERSIST_WORKER[持久化Worker<br/>批量写入]
    end

    subgraph "缓存层"
        LATEST_CACHE[最新K线缓存<br/>Redis]
        INDICATOR_CACHE[指标缓存<br/>Redis]
    end

    subgraph "存储层"
        CANDLE_REPO[K线仓储<br/>MySQL]
        TICKER_REPO[行情仓储<br/>MySQL]
    end

    OKX_API --> WEBSOCKET
    OKX_API --> REST_API
    BINANCE_API -.-> WEBSOCKET
    BINANCE_API -.-> REST_API

    WEBSOCKET --> CANDLE_SVC
    WEBSOCKET --> TICKER_SVC
    REST_API --> CANDLE_SVC

    CANDLE_SVC --> LATEST_CACHE
    CANDLE_SVC --> PERSIST_WORKER
    TICKER_SVC --> PERSIST_WORKER

    PERSIST_WORKER --> CANDLE_REPO
    PERSIST_WORKER --> TICKER_REPO

    LATEST_CACHE --> INDICATOR_CACHE

    style OKX_API fill:#ff6b6b
    style WEBSOCKET fill:#4ecdc4
    style CANDLE_SVC fill:#95e1d3
    style LATEST_CACHE fill:#fce4ec
    style CANDLE_REPO fill:#e0f2f1
```

## 6. 风控系统架构

```mermaid
graph TB
    subgraph "风控入口"
        RISK_SVC[RiskManagementService<br/>风控管理服务]
    end

    subgraph "风控策略"
        ORDER_POLICY[订单风控策略<br/>OrderRiskPolicy]
        POSITION_POLICY[持仓风控策略<br/>PositionRiskPolicy]
        ACCOUNT_POLICY[账户风控策略<br/>AccountRiskPolicy]
    end

    subgraph "风控检查"
        ORDER_RISK[订单风控<br/>Order Risk]
        POSITION_RISK[持仓风控<br/>Position Risk]
        ACCOUNT_RISK[账户风控<br/>Account Risk]
    end

    subgraph "数据源"
        POSITION_REPO[持仓仓储<br/>PositionRepository]
        ORDER_REPO[订单仓储<br/>OrderRepository]
        EXCHANGE_API[交易所API<br/>账户信息]
    end

    RISK_SVC --> ORDER_POLICY
    RISK_SVC --> POSITION_POLICY
    RISK_SVC --> ACCOUNT_POLICY

    ORDER_POLICY --> ORDER_RISK
    POSITION_POLICY --> POSITION_RISK
    ACCOUNT_POLICY --> ACCOUNT_RISK

    ORDER_RISK --> ORDER_REPO
    POSITION_RISK --> POSITION_REPO
    ACCOUNT_RISK --> EXCHANGE_API
    ACCOUNT_RISK --> POSITION_REPO

    style RISK_SVC fill:#e1f5ff
    style ORDER_POLICY fill:#fff4e1
    style ORDER_RISK fill:#ff6b6b
    style POSITION_REPO fill:#e0f2f1
```

## 7. 交易所API配置系统

```mermaid
graph TB
    subgraph "策略执行"
        STRATEGY_EXEC[StrategyExecutionService<br/>策略执行服务]
    end

    subgraph "API配置服务"
        EXCHANGE_API_SVC[ExchangeApiService<br/>API配置管理]
    end

    subgraph "订单执行"
        OKX_ORDER_SVC[OkxOrderService<br/>OKX订单服务]
    end

    subgraph "缓存层"
        REDIS_CACHE[Redis缓存<br/>strategy_api_config:ID]
    end

    subgraph "数据层"
        API_CONFIG_REPO[ExchangeApiConfigRepository<br/>API配置仓储]
        STRATEGY_API_REPO[StrategyApiConfigRepository<br/>关联仓储]
    end

    subgraph "数据库"
        EXCHANGE_API_TABLE[exchange_api_config<br/>API配置表]
        STRATEGY_API_TABLE[strategy_api_config<br/>关联表]
    end

    STRATEGY_EXEC --> EXCHANGE_API_SVC
    STRATEGY_EXEC --> OKX_ORDER_SVC

    EXCHANGE_API_SVC --> REDIS_CACHE
    EXCHANGE_API_SVC --> API_CONFIG_REPO
    EXCHANGE_API_SVC --> STRATEGY_API_REPO

    API_CONFIG_REPO --> EXCHANGE_API_TABLE
    STRATEGY_API_REPO --> STRATEGY_API_TABLE

    OKX_ORDER_SVC --> OKX_API[OKX交易所API]

    style STRATEGY_EXEC fill:#e1f5ff
    style EXCHANGE_API_SVC fill:#95e1d3
    style REDIS_CACHE fill:#fce4ec
    style EXCHANGE_API_TABLE fill:#e0f2f1
```

## 8. 核心模块职责说明

### 8.1 分层职责

| 层级 | 包名 | 职责 | 依赖方向 |
|------|------|------|----------|
| **入口层** | `rust-quant-cli` | 程序入口、命令行参数解析 | → orchestration |
| **编排层** | `orchestration` | 任务调度、工作流编排、事件驱动 | → services |
| **服务层** | `services` | 业务流程协调、事务管理 | → domain + infrastructure |
| **领域层** | `domain` | 业务实体、值对象、接口定义 | 无外部依赖 |
| **业务逻辑层** | `strategies`<br/>`indicators`<br/>`risk`<br/>`execution` | 策略实现、指标计算、风控逻辑、订单执行 | → domain |
| **基础设施层** | `infrastructure` | Repository实现、缓存、交易所适配器 | → domain |
| **核心层** | `core` | 配置、数据库连接池、Redis客户端、日志 | → common |
| **通用层** | `common` | 工具函数、通用类型、常量 | 无业务依赖 |
| **市场数据层** | `market` | WebSocket流、K线数据、行情数据 | → domain + core |
| **分析层** | `analytics` | 性能分析、报表生成 | → infrastructure |
| **AI分析层** | `ai-analysis` | 情绪分析、事件检测、影响预测 | → market |

### 8.2 关键模块

#### Domain（领域层）
- **entities/**: `Order`, `Position`, `StrategyConfig`, `Candle`, `ExchangeApiConfig`
- **value_objects/**: `Price`, `Volume`, `Signal`, `Leverage`, `Percentage`
- **enums/**: `OrderSide`, `OrderStatus`, `StrategyType`, `Timeframe`
- **traits/**: `CandleRepository`, `OrderRepository`, `Strategy`, `ExchangeAccount`

#### Services（服务层）
- **strategy/**: `StrategyExecutionService`, `StrategyConfigService`
- **exchange/**: `ExchangeApiService`, `OkxOrderService`
- **risk/**: `RiskManagementService`
- **market/**: `CandleService`, `TickerService`
- **trading/**: `OrderCreationService`

#### Infrastructure（基础设施层）
- **repositories/**: `SqlxCandleRepository`, `SqlxOrderRepository`, `SqlxStrategyConfigRepository`
- **cache/**: `RedisCache`, `InMemoryCache`, `TwoLevelCache`
- **exchanges/**: `OkxAdapter`, `ExchangeFactory`

#### Strategies（策略层）
- **framework/**: 策略框架、通用回测逻辑
- **implementations/**: Vegas、NWE等具体策略实现
- **backtesting/**: 回测引擎

#### Indicators（指标层）
- **trend/**: EMA, SMA, Vegas, NWE
- **momentum/**: MACD, RSI, KDJ
- **volatility/**: ATR, Bollinger Bands
- **volume/**: Volume Indicator
- **pattern/**: Support/Resistance, Market Structure

## 9. 数据流向

### 9.1 策略执行流程
```
CLI → Orchestration → Services → Strategies → Indicators → Domain
                                                      ↓
                                              Infrastructure → Database
```

### 9.2 市场数据流程
```
Exchange API → WebSocket → Market Service → Cache (Redis) → Repository → Database (MySQL)
```

### 9.3 订单执行流程
```
Strategy Signal → Risk Check → Exchange Service → API Config (Redis) → OKX API → Order Result → Database
```

## 10. 技术栈

- **语言**: Rust
- **异步运行时**: Tokio
- **数据库**: MySQL (sqlx)
- **缓存**: Redis
- **日志**: tracing
- **配置**: TOML
- **交易所SDK**: okx (自定义)
- **WebSocket**: tokio-tungstenite

