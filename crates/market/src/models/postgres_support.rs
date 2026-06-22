use anyhow::{anyhow, Result};
use once_cell::sync::OnceCell;
use sqlx::{postgres::PgPoolOptions, PgPool};
use std::collections::HashMap;

static QUANT_CORE_PG_POOL: OnceCell<PgPool> = OnceCell::new();

pub fn get_quant_core_postgres_pool() -> Result<&'static PgPool> {
    QUANT_CORE_PG_POOL.get_or_try_init(|| {
        let database_url = quant_core_database_url_from_env()?;

        PgPoolOptions::new()
            .max_connections(5)
            .connect_lazy(&database_url)
            .map_err(|e| anyhow!("failed to create quant_core Postgres pool: {}", e))
    })
}

fn quant_core_database_url_from_env() -> Result<String> {
    let envs: HashMap<String, String> = std::env::vars().collect();
    quant_core_database_url_from_map(&envs)
}

fn quant_core_database_url_from_map(envs: &HashMap<String, String>) -> Result<String> {
    if let Some(database_url) = non_empty_env(envs, "QUANT_CORE_DATABASE_URL")
        .or_else(|| non_empty_env(envs, "POSTGRES_QUANT_CORE_DATABASE_URL"))
    {
        return Ok(database_url.to_string());
    }

    let database_url = non_empty_env(envs, "DATABASE_URL").ok_or_else(|| {
        anyhow!(
            "missing quant_core Postgres database url; set QUANT_CORE_DATABASE_URL or POSTGRES_QUANT_CORE_DATABASE_URL"
        )
    })?;
    if !database_url_targets_quant_core(database_url) {
        return Err(anyhow!(
            "QUANT_CORE_DATABASE_URL must be set for quant_core Postgres access; DATABASE_URL points to a non-core database"
        ));
    }

    Ok(database_url.to_string())
}

fn non_empty_env<'a>(envs: &'a HashMap<String, String>, key: &str) -> Option<&'a str> {
    envs.get(key)
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
}

fn database_url_targets_quant_core(database_url: &str) -> bool {
    database_url
        .split('?')
        .next()
        .unwrap_or(database_url)
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .map(|database_name| database_name.eq_ignore_ascii_case("quant_core"))
        .unwrap_or(false)
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
    use super::{quant_core_database_url_from_map, quote_legacy_table_name};
    use std::collections::HashMap;

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

    #[test]
    fn quant_core_database_url_rejects_quant_web_fallback() {
        let envs = HashMap::from([(
            "DATABASE_URL".to_string(),
            "postgres://postgres:secret@localhost:5432/quant_web".to_string(),
        )]);

        let error =
            quant_core_database_url_from_map(&envs).expect_err("quant_web fallback must fail");
        assert!(
            error.to_string().contains("QUANT_CORE_DATABASE_URL"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn quant_core_database_url_prefers_explicit_core_url() {
        let envs = HashMap::from([
            (
                "QUANT_CORE_DATABASE_URL".to_string(),
                "postgres://postgres:secret@localhost:5432/quant_core".to_string(),
            ),
            (
                "DATABASE_URL".to_string(),
                "postgres://postgres:secret@localhost:5432/quant_web".to_string(),
            ),
        ]);

        assert_eq!(
            quant_core_database_url_from_map(&envs).expect("explicit quant_core url"),
            "postgres://postgres:secret@localhost:5432/quant_core"
        );
    }
}
