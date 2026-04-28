use anyhow::{anyhow, Context, Result};
use once_cell::sync::OnceCell;
use sqlx::{postgres::PgPoolOptions, PgPool};

static QUANT_CORE_PG_POOL: OnceCell<PgPool> = OnceCell::new();

pub fn get_quant_core_postgres_pool() -> Result<&'static PgPool> {
    QUANT_CORE_PG_POOL.get_or_try_init(|| {
        let database_url = std::env::var("QUANT_CORE_DATABASE_URL")
            .or_else(|_| std::env::var("POSTGRES_QUANT_CORE_DATABASE_URL"))
            .or_else(|_| std::env::var("DATABASE_URL"))
            .context(
                "missing Postgres database url; tried QUANT_CORE_DATABASE_URL, POSTGRES_QUANT_CORE_DATABASE_URL, DATABASE_URL",
            )?;

        PgPoolOptions::new()
            .max_connections(5)
            .connect_lazy(&database_url)
            .map_err(|e| anyhow!("failed to create quant_core Postgres pool: {}", e))
    })
}

pub fn quote_legacy_table_name(table_name: &str) -> Result<String> {
    if table_name
        .chars()
        .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || matches!(ch, '_' | '-'))
    {
        Ok(format!("\"{}\"", table_name))
    } else {
        Err(anyhow!("illegal legacy table name: {}", table_name))
    }
}

#[cfg(test)]
mod tests {
    use super::quote_legacy_table_name;

    #[test]
    fn quotes_legacy_sharded_table_names_for_postgres() {
        assert_eq!(
            quote_legacy_table_name("btc-usdt-swap_candles_4h").unwrap(),
            "\"btc-usdt-swap_candles_4h\""
        );
    }

    #[test]
    fn rejects_invalid_legacy_table_names() {
        assert!(quote_legacy_table_name("btc-usdt-swap;drop").is_err());
    }
}
