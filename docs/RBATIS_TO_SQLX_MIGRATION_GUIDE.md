# rbatis 到 sqlx 迁移指南

## 🎯 目标

将 `market` 包中的所有 `rbatis` 代码迁移到 `sqlx`。

---

## 📋 需要修改的文件

1. `crates/market/src/models/candles.rs`
2. `crates/market/src/models/tickers.rs`
3. `crates/market/src/models/tickers_volume.rs`
4. `crates/market/src/repositories/candle_service.rs`

---

## 🔄 迁移步骤

### **Step 1: 修改数据模型**

#### **原代码（rbatis）**:
```rust
use rbatis::RBatis;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CandlesModel {
    pub id: Option<i64>,
    pub inst_id: String,
    pub period: String,
    // ...
}
```

#### **新代码（sqlx）**:
```rust
use sqlx::FromRow;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, FromRow)]
pub struct CandlesModel {
    pub id: Option<i64>,
    pub inst_id: String,
    pub period: String,
    // ...
}
```

**关键改动**:
1. 移除 `use rbatis::RBatis`
2. 添加 `use sqlx::FromRow`
3. 在 struct 上添加 `FromRow` derive

---

### **Step 2: 修改 DateTime 类型**

#### **原代码（rbatis）**:
```rust
use rbatis::rbdc::DateTime;

pub struct CandlesModel {
    pub created_at: Option<DateTime>,
}
```

#### **新代码（sqlx）**:
```rust
use chrono::{DateTime, Utc};

pub struct CandlesModel {
    pub created_at: Option<DateTime<Utc>>,
}
```

---

### **Step 3: 重写查询方法**

#### **原代码（rbatis）**:
```rust
pub async fn insert(&self, rb: &RBatis) -> anyhow::Result<()> {
    rb.save(self, &[]).await?;
    Ok(())
}

pub async fn query_by_inst_id(rb: &RBatis, inst_id: &str) -> anyhow::Result<Vec<Self>> {
    let result = rb
        .query_decode("SELECT * FROM candles WHERE inst_id = ?", vec![inst_id.into()])
        .await?;
    Ok(result)
}
```

#### **新代码（sqlx）**:
```rust
use rust_quant_core::database::get_db_pool;

pub async fn insert(&self) -> anyhow::Result<()> {
    let pool = get_db_pool();
    
    sqlx::query!(
        r#"
        INSERT INTO candles (inst_id, period, o, h, l, c, vol, ts, confirm)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
        self.inst_id,
        self.period,
        self.o,
        self.h,
        self.l,
        self.c,
        self.vol,
        self.ts,
        self.confirm,
    )
    .execute(pool)
    .await?;
    
    Ok(())
}

pub async fn query_by_inst_id(inst_id: &str) -> anyhow::Result<Vec<Self>> {
    let pool = get_db_pool();
    
    let result = sqlx::query_as!(
        Self,
        r#"
        SELECT * FROM candles WHERE inst_id = ?
        "#,
        inst_id
    )
    .fetch_all(pool)
    .await?;
    
    Ok(result)
}
```

**关键改动**:
1. 移除 `rb: &RBatis` 参数
2. 使用 `get_db_pool()` 获取连接池
3. 使用 `sqlx::query!` 或 `sqlx::query_as!` 宏
4. 使用 `.bind()` 绑定参数

---

### **Step 4: 处理事务**

#### **原代码（rbatis）**:
```rust
let tx = rb.acquire_begin().await?;
tx.save(&model, &[]).await?;
tx.commit().await?;
```

#### **新代码（sqlx）**:
```rust
let pool = get_db_pool();
let mut tx = pool.begin().await?;

sqlx::query!("INSERT INTO ...")
    .execute(&mut *tx)
    .await?;

tx.commit().await?;
```

---

## 🔧 完整示例

### **修改前（rbatis）**:
```rust
// crates/market/src/models/candles.rs
use rbatis::RBatis;
use rbatis::rbdc::DateTime;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CandlesModel {
    pub id: Option<i64>,
    pub inst_id: String,
    pub created_at: Option<DateTime>,
}

impl CandlesModel {
    pub async fn save(&self, rb: &RBatis) -> anyhow::Result<()> {
        rb.save(self, &[]).await?;
        Ok(())
    }
}
```

### **修改后（sqlx）**:
```rust
// crates/market/src/models/candles.rs
use sqlx::FromRow;
use chrono::{DateTime, Utc};
use rust_quant_core::database::get_db_pool;

#[derive(Clone, Debug, Serialize, Deserialize, FromRow)]
pub struct CandlesModel {
    pub id: Option<i64>,
    pub inst_id: String,
    pub created_at: Option<DateTime<Utc>>,
}

impl CandlesModel {
    pub async fn save(&self) -> anyhow::Result<()> {
        let pool = get_db_pool();
        
        sqlx::query!(
            r#"
            INSERT INTO candles (inst_id, created_at)
            VALUES (?, ?)
            "#,
            self.inst_id,
            self.created_at,
        )
        .execute(pool)
        .await?;
        
        Ok(())
    }
}
```

---

## ⚠️ 注意事项

### **1. 字段映射**

sqlx 会自动映射字段名，但需要确保：
- 数据库字段名与 struct 字段名一致
- 或使用 `#[sqlx(rename = "db_field_name")]`

### **2. Option 类型**

sqlx 自动处理 NULL：
- `Option<T>` → 数据库 NULL
- `T` → 数据库 NOT NULL

### **3. 时间类型**

```rust
// rbatis
use rbatis::rbdc::DateTime;
pub created_at: DateTime;

// sqlx
use chrono::{DateTime, Utc};
pub created_at: DateTime<Utc>;
```

---

## 📝 修改清单

### **crates/market/src/models/candles.rs**

- [ ] 添加 `use sqlx::FromRow`
- [ ] 移除 `use rbatis::*`
- [ ] 修改 `DateTime` 类型
- [ ] 重写查询方法

### **crates/market/src/repositories/candle_service.rs**

- [ ] 移除 `rb: &RBatis` 参数
- [ ] 使用 `get_db_pool()`
- [ ] 重写所有 SQL 查询

---

## 🚀 开始迁移

```bash
# 1. 打开文件
code crates/market/src/models/candles.rs

# 2. 参考本指南逐步修改

# 3. 验证编译
cargo check --package rust-quant-market

# 4. 重复直到所有文件修复完成
```

**祝迁移顺利！** 🎯

