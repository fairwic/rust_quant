use super::{env_or_default, parse_symbol_list};

const EXCLUDED_SYMBOLS_ENV: &str = "MARKET_VELOCITY_BACKFILL_EXCLUDED_SYMBOLS";

pub(super) fn resolve_excluded_symbols(cli_symbols: Option<Vec<String>>) -> Vec<String> {
    cli_symbols.unwrap_or_else(|| parse_symbol_list(&env_or_default(EXCLUDED_SYMBOLS_ENV, "")))
}

pub(super) fn retain_legacy_owned_symbols(symbols: &mut Vec<String>, excluded_symbols: &[String]) {
    symbols.retain(|symbol| !excluded_symbols.contains(symbol));
}

#[cfg(test)]
mod tests {
    use super::super::parse_cli_args_from;
    use super::retain_legacy_owned_symbols;

    #[test]
    fn cli_accepts_exact_cutover_symbol_exclusions() {
        let args = parse_cli_args_from([
            "--enabled-strategy-symbols",
            "--exclude-symbols",
            "eth-usdt-swap, BTC-USDT-SWAP",
        ])
        .unwrap();
        assert_eq!(
            args.excluded_symbols,
            Some(vec![
                "BTC-USDT-SWAP".to_string(),
                "ETH-USDT-SWAP".to_string()
            ])
        );
    }

    #[test]
    fn filtering_removes_only_the_explicit_cutover_symbol() {
        let mut symbols = vec!["BTC-USDT-SWAP".to_string(), "ETH-USDT-SWAP".to_string()];
        retain_legacy_owned_symbols(&mut symbols, &["ETH-USDT-SWAP".to_string()]);
        assert_eq!(symbols, vec!["BTC-USDT-SWAP"]);
    }
}
