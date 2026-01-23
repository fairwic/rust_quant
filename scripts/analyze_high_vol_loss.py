import pymysql
import pandas as pd
import sys

# DB Config
DB_CONFIG = {
    'host': 'localhost',
    'port': 33306,
    'user': 'root',
    'password': 'example',
    'database': 'test',
    'cursorclass': pymysql.cursors.DictCursor
}

def analyze(back_test_id):
    conn = pymysql.connect(**DB_CONFIG)
    try:
        # 1. Fetch Log
        with conn.cursor() as cursor:
            cursor.execute("SELECT * FROM back_test_log WHERE id=%s", (back_test_id,))
            log = cursor.fetchone()
            if not log:
                print(f"Backtest {back_test_id} not found")
                return

        inst_id = log['inst_type']
        period = log['time']
        print(f"Analyzing Backtest {back_test_id}: {inst_id} {period}")

        # 2. Fetch Candles
        table_name = f"{inst_id.lower()}_candles_{period.lower()}"
        print(f"Fetching candles from {table_name}...")
        
        with conn.cursor() as cursor:
            # Fetch all to dataframe
            cursor.execute(f"SELECT ts, o, h, l, c FROM `{table_name}`")
            candles = cursor.fetchall()
            
        df_candles = pd.DataFrame(candles)
        df_candles['ts'] = pd.to_numeric(df_candles['ts'])
        df_candles['Date'] = pd.to_datetime(df_candles['ts'], unit='ms')
        # Calculate Amplitude
        for col in ['o', 'h', 'l', 'c']:
            df_candles[col] = pd.to_numeric(df_candles[col])
            
        df_candles['amplitude'] = (df_candles['h'] - df_candles['l']) / df_candles['l']
        
        # Index by timestamp for fast lookup
        # Note: Backtest Trade 'open_position_time' matches Candle 'ts' (Open Time) 
        # based on our investigation (Trade Open Price = Candle Close).
        df_candles.set_index('Date', inplace=True)

        # 3. Fetch Trades
        with conn.cursor() as cursor:
            cursor.execute(
                "SELECT * FROM back_test_detail WHERE back_test_id=%s ORDER BY open_position_time ASC",
                (back_test_id,)
            )
            trades = cursor.fetchall()

        print(f"Total Trades: {len(trades)}")
        
        # 4. Analyze
        high_vol_losses = []
        
        for t in trades:
            entry_time = pd.to_datetime(t['open_position_time'])
            pnl = float(t['profit_loss'])
            
            if pnl >= 0: continue # Skip wins
            
            if entry_time in df_candles.index:
                candle = df_candles.loc[entry_time]
                amp = candle['amplitude']
                
                if amp > 0.05:
                    mid_price = (candle['h'] + candle['l']) / 2
                    high_vol_losses.append({
                        'time': entry_time,
                        'type': t['option_type'],
                        'pnl': pnl,
                        'amplitude': amp,
                        'candle': candle,
                        'close_type': t['close_type'],
                        'result': t.get('signal_result', '')
                    })
        
        # 5. Report
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

    finally:
        conn.close()

if __name__ == "__main__":
    bid = 15650
    if len(sys.argv) > 1:
        bid = sys.argv[1]
    analyze(bid)
