import pandas as pd
import sys

from postgres_backtest import query_rows, quote_identifier

def analyze(back_test_id):
    back_test_id = int(back_test_id)

    log_rows = query_rows(f"SELECT * FROM back_test_log WHERE id = {back_test_id}")
    if not log_rows:
        print(f"Backtest {back_test_id} not found")
        return

    log = log_rows[0]
    inst_id = log['inst_type']
    period = log['time']
    print(f"Analyzing Backtest {back_test_id}: {inst_id} {period}")

    table_name = quote_identifier(f"{inst_id.lower()}_candles_{period.lower()}")
    print(f"Fetching candles from {table_name}...")

    candles = query_rows(f"SELECT ts, o, h, l, c FROM {table_name}")
    df_candles = pd.DataFrame(candles)
    df_candles['ts'] = pd.to_numeric(df_candles['ts'])
    df_candles['Date'] = pd.to_datetime(df_candles['ts'], unit='ms')
    for col in ['o', 'h', 'l', 'c']:
        df_candles[col] = pd.to_numeric(df_candles[col])

    df_candles['amplitude'] = (df_candles['h'] - df_candles['l']) / df_candles['l']
    df_candles.set_index('Date', inplace=True)

    trades = query_rows(
        "SELECT * FROM back_test_detail "
        f"WHERE back_test_id = {back_test_id} ORDER BY open_position_time ASC"
    )
    print(f"Total Trades: {len(trades)}")

    high_vol_losses = []
    for trade in trades:
        entry_time = pd.to_datetime(trade['open_position_time'])
        pnl = float(trade['profit_loss'])
        if pnl >= 0:
            continue

        if entry_time in df_candles.index:
            candle = df_candles.loc[entry_time]
            amp = candle['amplitude']

            if amp > 0.05:
                high_vol_losses.append({
                    'time': entry_time,
                    'type': trade['option_type'],
                    'pnl': pnl,
                    'amplitude': amp,
                    'candle': candle,
                    'close_type': trade['close_type'],
                    'result': trade.get('signal_result', '')
                })

    print(f"\nFound {len(high_vol_losses)} High Volatility (>5%) Loss Trades:")
    print("-" * 80)
    print(f"{'Time':<25} {'Type':<6} {'PnL':<10} {'Amp%':<8} {'CloseType':<15}")
    print("-" * 80)

    for item in high_vol_losses:
        print(f"{str(item['time']):<25} {item['type']:<6} {item['pnl']:<10.2f} {item['amplitude']*100:<8.2f} {item['close_type']:<15}")

    if high_vol_losses:
        avg_pnl = sum(i['pnl'] for i in high_vol_losses) / len(high_vol_losses)
        print("-" * 80)
        print(f"Average PnL of these trades: {avg_pnl:.2f}")

if __name__ == "__main__":
    bid = 15650
    if len(sys.argv) > 1:
        bid = sys.argv[1]
    analyze(bid)
