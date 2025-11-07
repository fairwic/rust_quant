# 🎨 Rust Quant 架构图 - Mermaid 版本

## 目录
1. [整体架构分层图](#1-整体架构分层图)
2. [包依赖关系图](#2-包依赖关系图)
3. [DDD分层架构](#3-ddd分层架构)
4. [策略执行流程](#4-策略执行流程)
5. [数据流图](#5-数据流图)
6. [技术栈架构](#6-技术栈架构)

---

## 1. 整体架构分层图

```mermaid
graph TB
    subgraph "应用层 Application Layer"
        CLI[rust-quant-cli<br/>命令行接口]
    end
    
    subgraph "编排层 Orchestration Layer"
        ORCH[rust-quant-orchestration<br/>任务调度/工作流]
    end
    
    subgraph "应用服务层 Application Services"
        SERV[rust-quant-services<br/>业务服务协调]
    end
    
    subgraph "业务层 Business Layer"
        STRAT[rust-quant-strategies<br/>策略引擎]
        RISK[rust-quant-risk<br/>风险管理]
        EXEC[rust-quant-execution<br/>订单执行]
        ANA[rust-quant-analytics<br/>分析报告]
        AI[rust-quant-ai-analysis<br/>AI分析]
    end
    
    subgraph "领域层 Domain Layer ⭐ DDD核心"
        DOMAIN[rust-quant-domain<br/>纯业务逻辑<br/>零外部依赖]
    end
    
    subgraph "基础设施层 Infrastructure Layer"
        INFRA[rust-quant-infrastructure<br/>数据访问/缓存<br/>实现domain接口]
    end
    
    subgraph "数据/计算层 Data & Computation Layer"
        MARKET[rust-quant-market<br/>市场数据]
        INDI[rust-quant-indicators<br/>技术指标]
    end
    
    subgraph "基础层 Foundation Layer"
        COMMON[rust-quant-common<br/>公共类型/工具]
        CORE[rust-quant-core<br/>配置/日志/数据库]
    end
    
    %% 依赖关系 (单向向下)
    CLI --> ORCH
    ORCH --> SERV
    SERV --> STRAT
    SERV --> RISK
    SERV --> EXEC
    
    STRAT --> DOMAIN
    STRAT --> INFRA
    STRAT --> INDI
    
    RISK --> DOMAIN
    RISK --> INFRA
    RISK --> MARKET
    
    EXEC --> DOMAIN
    EXEC --> INFRA
    EXEC --> RISK
    
    ANA --> DOMAIN
    ANA --> INFRA
    
    AI --> DOMAIN
    AI --> INFRA
    
    INFRA --> DOMAIN
    INFRA --> CORE
    INFRA --> COMMON
    
    INDI --> COMMON
    INDI --> DOMAIN
    
    MARKET --> DOMAIN
    MARKET --> CORE
    MARKET --> COMMON
    
    DOMAIN --> COMMON
    
    CORE --> COMMON
    
    style DOMAIN fill:#ff9999,stroke:#ff0000,stroke-width:3px
    style INFRA fill:#99ccff,stroke:#0066cc,stroke-width:3px
    style STRAT fill:#99ff99,stroke:#00cc00,stroke-width:3px
    style CLI fill:#ffcc99,stroke:#ff9900,stroke-width:2px
```

---

## 2. 包依赖关系图

```mermaid
graph LR
    subgraph "14 Packages"
        common[common<br/>✅]
        core[core<br/>✅]
        domain[domain<br/>✅ ⭐]
        infra[infrastructure<br/>✅ ⭐]
        market[market<br/>✅]
        indi[indicators<br/>✅]
        strat[strategies<br/>✅ ⭐⭐⭐]
        risk[risk<br/>✅]
        exec[execution<br/>🟡]
        orch[orchestration<br/>🟡]
        ana[analytics<br/>✅]
        ai[ai-analysis<br/>✅]
        serv[services<br/>🟡]
        cli[cli<br/>✅]
    end
    
    %% 依赖关系
    cli --> orch
    orch --> serv
    orch --> strat
    orch --> exec
    
    serv --> strat
    serv --> risk
    serv --> exec
    
    strat --> infra
    strat --> indi
    strat --> domain
    
    risk --> infra
    risk --> market
    risk --> domain
    
    exec --> infra
    exec --> risk
    exec --> domain
    
    ana --> infra
    ana --> domain
    
    ai --> infra
    ai --> domain
    
    infra --> domain
    infra --> core
    infra --> common
    
    indi --> domain
    indi --> common
    
    market --> domain
    market --> core
    market --> common
    
    domain --> common
    
    core --> common
    
    style domain fill:#ff9999,stroke:#ff0000,stroke-width:3px
    style infra fill:#99ccff,stroke:#0066cc,stroke-width:3px
    style strat fill:#99ff99,stroke:#00cc00,stroke-width:3px
    style exec fill:#99ff99,stroke:#00cc00,stroke-width:2px
    style orch fill:#99ff99,stroke:#00cc00,stroke-width:2px
    style serv fill:#99ff99,stroke:#00cc00,stroke-width:2px
```

**图例**:
- ✅ = 编译通过 (14个，100% ⭐⭐⭐⭐⭐)
- ⭐ = DDD核心
- ⭐⭐⭐ = 本次重构重点

---

## 3. DDD分层架构

```mermaid
graph TB
    subgraph "表现层 Presentation"
        CLI[CLI Commands<br/>命令行界面]
    end
    
    subgraph "应用层 Application"
        ORCH[Orchestration<br/>任务编排]
        SERV[Services<br/>应用服务]
    end
    
    subgraph "领域层 Domain ⭐"
        ENTITIES[Entities<br/>实体]
        VO[Value Objects<br/>值对象]
        ENUMS[Enums<br/>枚举]
        TRAITS[Domain Traits<br/>领域接口]
    end
    
    subgraph "业务逻辑层 Business Logic"
        STRAT_IMPL[Strategy Implementations<br/>策略实现]
        RISK_IMPL[Risk Policies<br/>风险策略]
        INDI_CALC[Indicator Calculations<br/>指标计算]
    end
    
    subgraph "基础设施层 Infrastructure"
        REPO[Repositories<br/>仓储实现]
        CACHE[Cache<br/>缓存]
        MSG[Messaging<br/>消息]
    end
    
    subgraph "数据层 Data"
        DB[(MySQL<br/>数据库)]
        REDIS[(Redis<br/>缓存)]
        API[External APIs<br/>外部API]
    end
    
    %% 依赖关系
    CLI --> ORCH
    CLI --> SERV
    
    ORCH --> STRAT_IMPL
    SERV --> STRAT_IMPL
    SERV --> RISK_IMPL
    
    STRAT_IMPL --> ENTITIES
    STRAT_IMPL --> VO
    STRAT_IMPL --> TRAITS
    STRAT_IMPL --> INDI_CALC
    
    RISK_IMPL --> ENTITIES
    RISK_IMPL --> ENUMS
    
    INDI_CALC --> VO
    
    STRAT_IMPL --> REPO
    RISK_IMPL --> REPO
    
    REPO -.实现.-> TRAITS
    CACHE -.支持.-> REPO
    
    REPO --> DB
    CACHE --> REDIS
    REPO --> API
    
    style ENTITIES fill:#ff9999,stroke:#ff0000,stroke-width:3px
    style VO fill:#ff9999,stroke:#ff0000,stroke-width:3px
    style TRAITS fill:#ff9999,stroke:#ff0000,stroke-width:3px
    style REPO fill:#99ccff,stroke:#0066cc,stroke-width:3px
```

---

## 4. 策略执行流程

```mermaid
sequenceDiagram
    participant User as 用户/定时任务
    participant CLI as CLI
    participant Orch as Orchestration
    participant StratMgr as Strategy Manager
    participant Strategy as Strategy Implementation
    participant Indi as Indicators
    participant Infra as Infrastructure
    participant Risk as Risk Manager
    participant Exec as Execution Engine
    participant OKX as OKX Exchange
    
    User->>CLI: 启动策略
    CLI->>Orch: 调度任务
    Orch->>StratMgr: 加载策略配置
    
    Note over StratMgr: 策略类型: Vegas/NWE/MACD-KDJ
    
    StratMgr->>Infra: 获取K线数据
    Infra-->>StratMgr: 返回历史K线
    
    StratMgr->>Strategy: 执行策略分析
    
    Strategy->>Indi: 计算技术指标
    Note over Indi: EMA, RSI, MACD, KDJ<br/>Vegas, NWE, ATR等
    Indi-->>Strategy: 返回指标值
    
    Strategy->>Strategy: 生成交易信号
    Note over Strategy: SignalResult<br/>Long/Short/Hold
    
    Strategy-->>StratMgr: 返回信号
    
    alt 有交易信号
        StratMgr->>Risk: 风险检查
        Note over Risk: 仓位限制<br/>止损止盈<br/>最大回撤
        
        alt 风险通过
            Risk-->>StratMgr: 通过
            StratMgr->>Exec: 创建订单
            
            Exec->>Exec: 订单管理
            Note over Exec: 订单类型<br/>价格计算<br/>数量计算
            
            Exec->>OKX: 提交订单
            OKX-->>Exec: 订单确认
            
            Exec->>Infra: 保存订单记录
            Exec-->>StratMgr: 执行完成
        else 风险拒绝
            Risk-->>StratMgr: 拒绝交易
        end
    else 无交易信号
        StratMgr->>StratMgr: 继续监控
    end
    
    StratMgr-->>Orch: 任务完成
    Orch-->>CLI: 返回结果
    CLI-->>User: 显示状态
```

---

## 5. 数据流图

```mermaid
graph LR
    subgraph "数据源 Data Sources"
        OKX_WS[OKX WebSocket<br/>实时行情]
        OKX_API[OKX REST API<br/>历史数据]
        NEWS[News APIs<br/>新闻资讯]
    end
    
    subgraph "数据采集 Data Collection"
        WS_SVC[WebSocket Service<br/>实时订阅]
        API_SVC[API Service<br/>定时拉取]
        NEWS_SVC[News Collector<br/>新闻采集]
    end
    
    subgraph "数据存储 Data Storage"
        MYSQL[(MySQL<br/>K线/订单/持仓)]
        REDIS[(Redis<br/>缓存/实时数据)]
    end
    
    subgraph "数据处理 Data Processing"
        NORM[Data Normalization<br/>数据标准化]
        VALID[Data Validation<br/>数据验证]
        CACHE_MGR[Cache Manager<br/>缓存管理]
    end
    
    subgraph "业务层 Business Layer"
        INDI_ENG[Indicator Engine<br/>指标计算]
        STRAT_ENG[Strategy Engine<br/>策略引擎]
        RISK_ENG[Risk Engine<br/>风险引擎]
    end
    
    subgraph "执行层 Execution"
        ORDER_MGR[Order Manager<br/>订单管理]
        POS_MGR[Position Manager<br/>持仓管理]
    end
    
    subgraph "输出 Output"
        TRADE[Trade Execution<br/>交易执行]
        REPORT[Reports<br/>报告]
        ALERT[Alerts<br/>告警]
    end
    
    %% 数据流
    OKX_WS --> WS_SVC
    OKX_API --> API_SVC
    NEWS --> NEWS_SVC
    
    WS_SVC --> NORM
    API_SVC --> NORM
    NEWS_SVC --> NORM
    
    NORM --> VALID
    VALID --> MYSQL
    VALID --> REDIS
    
    MYSQL --> CACHE_MGR
    REDIS --> CACHE_MGR
    
    CACHE_MGR --> INDI_ENG
    CACHE_MGR --> STRAT_ENG
    
    INDI_ENG --> STRAT_ENG
    STRAT_ENG --> RISK_ENG
    
    RISK_ENG --> ORDER_MGR
    ORDER_MGR --> POS_MGR
    
    ORDER_MGR --> TRADE
    POS_MGR --> REPORT
    RISK_ENG --> ALERT
    
    TRADE --> OKX_API
    
    style MYSQL fill:#ff9999,stroke:#ff0000,stroke-width:2px
    style REDIS fill:#ff9999,stroke:#ff0000,stroke-width:2px
    style STRAT_ENG fill:#99ff99,stroke:#00cc00,stroke-width:3px
```

---

## 6. 技术栈架构

```mermaid
graph TB
    subgraph "前端展示 Frontend"
        CLI_UI[CLI Interface<br/>终端界面]
    end
    
    subgraph "应用层 Application"
        RUST_APP[Rust Application<br/>主程序]
    end
    
    subgraph "业务逻辑 Business Logic"
        STRAT_MOD[Strategies Module<br/>策略模块]
        RISK_MOD[Risk Module<br/>风险模块]
        INDI_MOD[Indicators Module<br/>指标模块]
    end
    
    subgraph "核心框架 Core Framework"
        TOKIO[Tokio<br/>异步运行时]
        SQLX[SQLx<br/>数据库ORM]
        REDIS_RS[Redis-rs<br/>Redis客户端]
        TRACING[Tracing<br/>日志框架]
    end
    
    subgraph "技术指标库 TA Libraries"
        TA_LIB[ta<br/>技术分析库]
        CUSTOM[Custom Indicators<br/>自定义指标]
    end
    
    subgraph "外部服务 External Services"
        OKX_SDK[OKX SDK<br/>交易所SDK]
        AI_API[AI APIs<br/>AI服务]
    end
    
    subgraph "数据存储 Data Storage"
        MYSQL_DB[(MySQL 8.0<br/>主数据库)]
        REDIS_DB[(Redis<br/>缓存/队列)]
    end
    
    %% 技术栈关系
    CLI_UI --> RUST_APP
    RUST_APP --> STRAT_MOD
    RUST_APP --> RISK_MOD
    
    STRAT_MOD --> INDI_MOD
    STRAT_MOD --> TOKIO
    STRAT_MOD --> SQLX
    
    INDI_MOD --> TA_LIB
    INDI_MOD --> CUSTOM
    
    RISK_MOD --> SQLX
    RISK_MOD --> REDIS_RS
    
    SQLX --> MYSQL_DB
    REDIS_RS --> REDIS_DB
    
    RUST_APP --> OKX_SDK
    RUST_APP --> AI_API
    RUST_APP --> TRACING
    
    style RUST_APP fill:#ff9999,stroke:#ff0000,stroke-width:3px
    style TOKIO fill:#99ccff,stroke:#0066cc,stroke-width:2px
    style SQLX fill:#99ccff,stroke:#0066cc,stroke-width:2px
```

---

## 7. 核心模块详细结构

### 7.1 Strategies 包内部结构

```mermaid
graph TB
    subgraph "rust-quant-strategies"
        subgraph "Framework 框架层"
            TRAIT[Strategy Trait<br/>策略接口]
            MGR[Strategy Manager<br/>策略管理器]
            REG[Strategy Registry<br/>策略注册]
            COMMON[Strategy Common<br/>通用逻辑]
            TYPES[Types<br/>类型定义]
        end
        
        subgraph "Adapters 适配器层"
            CANDLE_ADP[Candle Adapter<br/>K线适配器<br/>⭐解决孤儿规则]
        end
        
        subgraph "Implementations 实现层"
            VEGAS[Vegas Strategy<br/>Vegas策略]
            NWE[NWE Strategy<br/>NWE策略]
            MACD_KDJ[MACD-KDJ Strategy<br/>MACD-KDJ策略]
            COMP[Comprehensive Strategy<br/>综合策略]
            SQUEEZE[Squeeze Strategy<br/>挤压策略]
            ENGULF[Engulfing Strategy<br/>吞没策略]
        end
        
        subgraph "Config 配置层"
            STRAT_CFG[Strategy Config<br/>策略配置]
            RISK_CFG[Risk Config<br/>风控配置]
            COMPAT[Config Compat<br/>兼容层]
        end
        
        MGR --> TRAIT
        MGR --> REG
        REG --> VEGAS
        REG --> NWE
        
        VEGAS --> TRAIT
        NWE --> TRAIT
        MACD_KDJ --> TRAIT
        COMP --> TRAIT
        
        VEGAS --> COMMON
        NWE --> COMMON
        COMP --> CANDLE_ADP
        
        VEGAS --> STRAT_CFG
        NWE --> STRAT_CFG
        
        COMMON --> TYPES
        COMMON --> RISK_CFG
    end
    
    style TRAIT fill:#ff9999,stroke:#ff0000,stroke-width:2px
    style CANDLE_ADP fill:#99ff99,stroke:#00cc00,stroke-width:3px
    style MGR fill:#99ccff,stroke:#0066cc,stroke-width:2px
```

### 7.2 Indicators 包内部结构

```mermaid
graph TB
    subgraph "rust-quant-indicators"
        subgraph "Trend 趋势指标"
            EMA[EMA<br/>指数移动平均]
            SMA[SMA<br/>简单移动平均]
            VEGAS_IND[Vegas Indicators<br/>Vegas指标系统]
            NWE_IND[NWE Indicators<br/>NWE指标系统<br/>⭐新增模块]
        end
        
        subgraph "Momentum 动量指标"
            RSI[RSI<br/>相对强弱指标]
            MACD[MACD<br/>指数平滑异同]
            KDJ[KDJ<br/>随机指标]
        end
        
        subgraph "Volatility 波动率指标"
            ATR[ATR<br/>真实波幅]
            BB[Bollinger Bands<br/>布林带]
            ATR_SL[ATR Stop Loss<br/>ATR止损]
        end
        
        subgraph "Volume 成交量指标"
            VOL[Volume Indicators<br/>成交量指标]
        end
        
        subgraph "Pattern 形态指标"
            ENGULF_PAT[Engulfing Pattern<br/>吞没形态]
            HAMMER[Hammer Pattern<br/>锤子线]
            FVG[Fair Value Gap<br/>公允价值缺口]
        end
        
        VEGAS_IND --> EMA
        NWE_IND --> EMA
        NWE_IND --> RSI
        NWE_IND --> MACD
        
        ATR_SL --> ATR
        
        BB --> SMA
    end
    
    style NWE_IND fill:#99ff99,stroke:#00cc00,stroke-width:3px
    style EMA fill:#99ccff,stroke:#0066cc,stroke-width:2px
```

### 7.3 Infrastructure 包内部结构

```mermaid
graph TB
    subgraph "rust-quant-infrastructure"
        subgraph "Repositories 仓储层"
            CANDLE_REPO[Candle Repository<br/>K线仓储]
            STRAT_CFG_REPO[Strategy Config Repo<br/>策略配置仓储]
            ORDER_REPO[Order Repository<br/>订单仓储]
        end
        
        subgraph "Cache 缓存层"
            STRAT_CACHE[Strategy Cache<br/>策略缓存]
            IND_CACHE[Indicator Cache<br/>指标缓存]
            VEGAS_CACHE[Vegas Indicator Cache<br/>Vegas指标缓存]
            NWE_CACHE[NWE Indicator Cache<br/>NWE指标缓存]
            EMA_CACHE[EMA Cache<br/>EMA缓存]
        end
        
        subgraph "Messaging 消息层"
            MSG[Message Queue<br/>消息队列]
        end
        
        CANDLE_REPO -.实现.-> DOMAIN_TRAIT[Domain Traits<br/>领域接口]
        STRAT_CFG_REPO -.实现.-> DOMAIN_TRAIT
        
        STRAT_CACHE --> REDIS_CLIENT[Redis Client<br/>Redis客户端]
        IND_CACHE --> REDIS_CLIENT
        VEGAS_CACHE --> REDIS_CLIENT
        NWE_CACHE --> REDIS_CLIENT
        EMA_CACHE --> REDIS_CLIENT
        
        CANDLE_REPO --> SQLX_POOL[SQLx Pool<br/>数据库连接池]
        STRAT_CFG_REPO --> SQLX_POOL
    end
    
    style DOMAIN_TRAIT fill:#ff9999,stroke:#ff0000,stroke-width:3px
    style CANDLE_REPO fill:#99ccff,stroke:#0066cc,stroke-width:2px
```

---

## 8. 回测流程

```mermaid
sequenceDiagram
    participant User as 用户
    participant CLI as CLI
    participant BackTest as Backtest Executor
    participant Candles as Candle Repository
    participant Strategy as Strategy
    participant Indicators as Indicators
    participant Risk as Risk Manager
    participant Logger as BackTest Logger
    
    User->>CLI: 启动回测
    Note over User,CLI: 参数: 策略/币对/周期/时间范围
    
    CLI->>BackTest: 初始化回测
    BackTest->>Candles: 加载历史K线
    Candles-->>BackTest: 返回K线数组
    
    Note over BackTest: 遍历每根K线
    
    loop 每根K线
        BackTest->>Strategy: 分析K线
        Strategy->>Indicators: 计算指标
        Indicators-->>Strategy: 返回指标值
        
        Strategy->>Strategy: 生成信号
        Strategy-->>BackTest: 返回信号
        
        alt 有交易信号
            BackTest->>Risk: 风险检查
            
            alt 风险通过
                BackTest->>BackTest: 模拟开仓
                Note over BackTest: 记录:<br/>- 开仓价格<br/>- 仓位大小<br/>- 止损止盈
                BackTest->>Logger: 记录交易
            else 风险拒绝
                BackTest->>Logger: 记录拒绝原因
            end
        end
        
        alt 有持仓
            BackTest->>BackTest: 检查止损止盈
            
            alt 触发止损/止盈
                BackTest->>BackTest: 模拟平仓
                BackTest->>Logger: 记录交易
            end
        end
    end
    
    BackTest->>BackTest: 计算回测指标
    Note over BackTest: - 总收益率<br/>- 最大回撤<br/>- 胜率<br/>- 盈亏比<br/>- 交易次数
    
    BackTest->>Logger: 保存回测报告
    Logger-->>BackTest: 保存完成
    
    BackTest-->>CLI: 返回结果
    CLI-->>User: 显示报告
```

---

## 9. 风险管理流程

```mermaid
graph TB
    START[开始] --> GET_SIGNAL[获取交易信号]
    
    GET_SIGNAL --> CHECK1{检查1:<br/>账户资金}
    
    CHECK1 -->|不足| REJECT1[拒绝: 资金不足]
    CHECK1 -->|充足| CHECK2{检查2:<br/>仓位限制}
    
    CHECK2 -->|超限| REJECT2[拒绝: 仓位超限]
    CHECK2 -->|未超限| CHECK3{检查3:<br/>单日交易次数}
    
    CHECK3 -->|超限| REJECT3[拒绝: 交易频繁]
    CHECK3 -->|未超限| CHECK4{检查4:<br/>最大回撤}
    
    CHECK4 -->|超限| REJECT4[拒绝: 回撤过大]
    CHECK4 -->|未超限| CHECK5{检查5:<br/>止损止盈设置}
    
    CHECK5 -->|无效| REJECT5[拒绝: 风控参数无效]
    CHECK5 -->|有效| CALC_SIZE[计算仓位大小]
    
    CALC_SIZE --> CALC_STOP[计算止损止盈]
    
    CALC_STOP --> VALIDATE{验证订单参数}
    
    VALIDATE -->|失败| REJECT6[拒绝: 订单参数无效]
    VALIDATE -->|成功| APPROVE[批准交易]
    
    APPROVE --> LOG[记录风控日志]
    LOG --> END[结束]
    
    REJECT1 --> LOG
    REJECT2 --> LOG
    REJECT3 --> LOG
    REJECT4 --> LOG
    REJECT5 --> LOG
    REJECT6 --> LOG
    
    style START fill:#99ff99,stroke:#00cc00,stroke-width:2px
    style APPROVE fill:#99ff99,stroke:#00cc00,stroke-width:3px
    style REJECT1 fill:#ff9999,stroke:#ff0000,stroke-width:2px
    style REJECT2 fill:#ff9999,stroke:#ff0000,stroke-width:2px
    style REJECT3 fill:#ff9999,stroke:#ff0000,stroke-width:2px
    style REJECT4 fill:#ff9999,stroke:#ff0000,stroke-width:2px
    style REJECT5 fill:#ff9999,stroke:#ff0000,stroke-width:2px
    style REJECT6 fill:#ff9999,stroke:#ff0000,stroke-width:2px
```

---

## 10. 适配器模式（解决孤儿规则）

```mermaid
classDiagram
    class CandlesEntity {
        +String inst_id
        +String bar
        +String ts
        +String o
        +String h
        +String l
        +String c
        +String vol
    }
    
    class CandleAdapter {
        +f64 open
        +f64 high
        +f64 low
        +f64 close
        +f64 volume
        +high() f64
        +low() f64
        +close() f64
        +open() f64
        +volume() f64
    }
    
    class High {
        <<trait>>
        +high() f64
    }
    
    class Low {
        <<trait>>
        +low() f64
    }
    
    class Close {
        <<trait>>
        +close() f64
    }
    
    class Open {
        <<trait>>
        +open() f64
    }
    
    class Volume {
        <<trait>>
        +volume() f64
    }
    
    class TA_Library {
        <<external>>
        使用 High, Low, Close
    }
    
    CandlesEntity ..> CandleAdapter : adapt()
    CandleAdapter ..|> High
    CandleAdapter ..|> Low
    CandleAdapter ..|> Close
    CandleAdapter ..|> Open
    CandleAdapter ..|> Volume
    
    TA_Library ..> High
    TA_Library ..> Low
    TA_Library ..> Close
    
    note for CandleAdapter "⭐ 适配器模式解决孤儿规则\n本地类型实现外部trait"
```

---

## 11. 配置管理流程

```mermaid
graph LR
    subgraph "配置源 Config Sources"
        ENV[环境变量<br/>.env]
        FILE[配置文件<br/>config/*.toml]
        DB[(数据库<br/>动态配置)]
    end
    
    subgraph "配置加载 Config Loading"
        LOADER[Config Loader<br/>配置加载器]
        VALIDATOR[Config Validator<br/>配置验证器]
    end
    
    subgraph "配置类型 Config Types"
        DB_CFG[Database Config<br/>数据库配置]
        REDIS_CFG[Redis Config<br/>Redis配置]
        STRAT_CFG[Strategy Config<br/>策略配置]
        RISK_CFG[Risk Config<br/>风控配置]
        LOG_CFG[Log Config<br/>日志配置]
    end
    
    subgraph "配置使用 Config Usage"
        CORE[Core Module<br/>核心模块]
        STRAT[Strategies<br/>策略模块]
        RISK_MOD[Risk Module<br/>风险模块]
    end
    
    ENV --> LOADER
    FILE --> LOADER
    DB --> LOADER
    
    LOADER --> VALIDATOR
    
    VALIDATOR --> DB_CFG
    VALIDATOR --> REDIS_CFG
    VALIDATOR --> STRAT_CFG
    VALIDATOR --> RISK_CFG
    VALIDATOR --> LOG_CFG
    
    DB_CFG --> CORE
    REDIS_CFG --> CORE
    STRAT_CFG --> STRAT
    RISK_CFG --> RISK_MOD
    LOG_CFG --> CORE
    
    style VALIDATOR fill:#99ff99,stroke:#00cc00,stroke-width:2px
    style STRAT_CFG fill:#ff9999,stroke:#ff0000,stroke-width:2px
```

---

## 使用说明

### 如何使用这些图表

1. **在线查看**: 
   - GitHub、GitLab 会自动渲染 Mermaid 图
   - VS Code 安装 Mermaid 插件

2. **导出图片**:
   ```bash
   # 使用 mermaid-cli
   npm install -g @mermaid-js/mermaid-cli
   mmdc -i ARCHITECTURE_DIAGRAMS.md -o architecture.png
   ```

3. **在线编辑**:
   - https://mermaid.live/
   - 复制代码在线编辑和导出

### 图表说明

| 图表 | 用途 | 受众 |
|------|------|------|
| 整体架构分层图 | 了解系统整体结构 | 所有人 |
| 包依赖关系图 | 了解包之间依赖 | 开发者 |
| DDD分层架构 | 了解DDD设计 | 架构师 |
| 策略执行流程 | 了解业务流程 | 开发者/运维 |
| 数据流图 | 了解数据流向 | 开发者 |
| 技术栈架构 | 了解技术选型 | 架构师 |
| 回测流程 | 了解回测机制 | 量化研究员 |
| 风险管理流程 | 了解风控逻辑 | 风控人员 |
| 适配器模式 | 了解设计模式 | 开发者 |

---

## 架构特点总结

### ✅ 优点

1. **清晰的分层**
   - 单向依赖
   - 职责明确
   - 易于理解

2. **DDD设计**
   - domain 零外部依赖
   - infrastructure 实现接口
   - 符合Clean Architecture

3. **适配器模式**
   - 解决孤儿规则
   - 标准解决方案
   - 可复用设计

4. **可扩展性**
   - 策略可插拔
   - 指标可复用
   - 风控可配置

5. **高性能**
   - 异步IO (Tokio)
   - Redis缓存
   - 连接池管理

### 🎯 设计原则

- ✅ 单一职责 (SRP)
- ✅ 开闭原则 (OCP)
- ✅ 依赖倒置 (DIP)
- ✅ 接口隔离 (ISP)
- ✅ DRY (Don't Repeat Yourself)

---

**Rust Quant v0.3.0 - 架构可视化** 🎨

*更新时间: 2025-11-07*

