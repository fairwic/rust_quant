import pymysql
import pandas as pd
import mplfinance as mpf
import sys
import os
import numpy as np
import json
import matplotlib.pyplot as plt

# DB Config
DB_CONFIG = {
    'host': 'localhost',
    'port': 33306,
    'user': 'root',
    'password': 'example',
    'database': 'test',
    'cursorclass': pymysql.cursors.DictCursor
}

def fetch_data(back_test_id):
    print(f"Fetching data for Backtest ID: {back_test_id}")
    conn = pymysql.connect(**DB_CONFIG)
    try:
        # 1. Fetch Log
        with conn.cursor() as cursor:
            cursor.execute("SELECT * FROM back_test_log WHERE id=%s", (back_test_id,))
            log = cursor.fetchone()
            if not log:
                print(f"Backtest {back_test_id} not found")
                return None, None, None
            
        inst_id = log['inst_type']
        period = log['time']
        
        # 2. Fetch Candles
        table_name = f"{inst_id.lower()}_candles_{period.lower()}"
        start_ts = log['kline_start_time']
        end_ts = log['kline_end_time']
        
        query_candles = f"SELECT ts, o, h, l, c FROM `{table_name}` WHERE ts >= %s AND ts <= %s ORDER BY ts ASC"
        
        with conn.cursor() as cursor:
            cursor.execute(query_candles, (start_ts, end_ts))
            candle_rows = cursor.fetchall()
            
        df_candles = pd.DataFrame(candle_rows)
        
        # 3. Fetch Trades
        query_trades = "SELECT * FROM back_test_detail WHERE back_test_id=%s ORDER BY open_position_time ASC"
        with conn.cursor() as cursor:
            cursor.execute(query_trades, (back_test_id,))
            trade_rows = cursor.fetchall()
            
        df_trades = pd.DataFrame(trade_rows)
        
        return df_candles, df_trades, log
        
    finally:
        conn.close()

def parse_signals(df_trades):
    stats = {} # { SignalName: {'win': 0, 'loss': 0, 'profit': 0.0} }
    
    for _, trade in df_trades.iterrows():
        try:
            pnl = float(trade['profit_loss'])
            is_win = pnl > 0
            # Parse signal_result
            # Format: [["Name", {Details}], ...]
            sig_res_str = trade['signal_result']
            if not sig_res_str:
                continue
            
            sig_list = json.loads(sig_res_str)
            trade_type = trade['option_type'] # 'long' or 'short'
            
            for item in sig_list:
                # item is [Name, Dict]
                if len(item) < 2: continue
                sig_name = item[0]
                details = item[1]
                
                # Check contribution
                # Recursively look for 'is_long_signal' / 'is_short_signal' in values
                contributed = False
                
                # Helper to recursive find signal flag
                def check_flag(d, key):
                    if isinstance(d, dict):
                        if d.get(key) is True: return True
                        for v in d.values():
                            if check_flag(v, key): return True
                    return False

                if trade_type == 'long':
                    if check_flag(details, 'is_long_signal'): contributed = True
                elif trade_type == 'short':
                    if check_flag(details, 'is_short_signal'): contributed = True
                
                # Some signals might not have flags but are present (weight based). 
                # Assuming presence implies contribution for now if detailed flag is missing/complex.
                # But strictly, we should check flags.
                # Exception: "VolumeTrend" detail structure {"Volume": {"is_increasing":...}} might not have 'is_long_signal' directly?
                # Let's check the sample output provided earlier.
                # VolumeTrend: {"Volume":{"is_increasing":false,"ratio":...}} -> No is_long_signal.
                # EmaTrend: {"EmaTouchTrend":{"is_long_signal":true...}} -> Yes.
                
                # If specifically Rsi/Bolling/Engulfing/EmaTrend, they have explicit flags.
                # For others, we might assume they contributed if they appear in this 'result' list AND the trade happened.
                # BUT 'signal_result' lists ALL checks. We need to know if it was POSITIVE.
                
                # Custom checks for known types without standard flags
                if sig_name == 'VolumeTrend': 
                     # Volume usually validates trend, hard to say "is_long". 
                     # Strategy uses it as a weight. If it's in the list, it was evaluated.
                     # Let's count it if it's strictly "True" for the direction? 
                     # Actually without weight detailed log, let's assume if 'is_increasing' is often the signal?
                     # Let's skip obscure ones and focus on explicit 'is_X_signal'.
                     pass
                
                # Re-eval contribution based on explicit flags
                if not contributed:
                    # Maybe it's a signal that doesn't use the flag convention?
                    pass
                
                if contributed:
                    if sig_name not in stats: stats[sig_name] = {'win': 0, 'loss': 0, 'profit': 0.0}
                    if is_win:
                        stats[sig_name]['win'] += 1
                        stats[sig_name]['profit'] += pnl
                    else:
                        stats[sig_name]['loss'] += 1
                        stats[sig_name]['profit'] += pnl
                        
        except Exception as e:
            # print(f"Error parsing trade {trade['id']}: {e}")
            pass
            
    return stats

def plot_stats_chart(back_test_id, stats):
    if not stats:
        print("No indicator stats derived.")
        return

    names = sorted(stats.keys())
    wins = [stats[n]['win'] for n in names]
    losses = [stats[n]['loss'] for n in names]
    profits = [stats[n]['profit'] for n in names]
    
    x = np.arange(len(names))
    width = 0.35
    
    fig, ax1 = plt.subplots(figsize=(12, 6))
    
    # Bar chart for Win/Loss counts
    rects1 = ax1.bar(x - width/2, wins, width, label='Wins', color='#2ebd85')
    rects2 = ax1.bar(x + width/2, losses, width, label='Losses', color='#f6465d')
    
    ax1.set_ylabel('Trade Count')
    ax1.set_title(f'Indicator Performance Stats (Backtest {back_test_id})')
    ax1.set_xticks(x)
    ax1.set_xticklabels(names, rotation=45, ha='right')
    ax1.legend(loc='upper left')
    
    # Secondary Y for Profit
    ax2 = ax1.twinx()
    ax2.plot(x, profits, color='blue', marker='o', linestyle='-', linewidth=2, label='Total Profit')
    ax2.set_ylabel('Total Profit (USDT)')
    ax2.legend(loc='upper right')
    
    # Add values
    ax1.bar_label(rects1, padding=3)
    ax1.bar_label(rects2, padding=3)
    
    plt.tight_layout()
    output_path = f"dist/vegas_backtest_stats_{back_test_id}.png"
    plt.savefig(output_path)
    print(f"Generated stats chart to {output_path}")

def plot_candle_chart(df_candles, df_trades, log, back_test_id):
    # --- Process Candles ---
    for col in ['o', 'h', 'l', 'c']:
        df_candles[col] = pd.to_numeric(df_candles[col], errors='coerce')
        
    df_candles['ts'] = pd.to_numeric(df_candles['ts'])
    df_candles['Date'] = pd.to_datetime(df_candles['ts'], unit='ms')
    df_candles.set_index('Date', inplace=True)
    df_candles.rename(columns={'o': 'Open', 'h': 'High', 'l': 'Low', 'c': 'Close'}, inplace=True)
    df_candles = df_candles.dropna()

    # --- Indicators ---
    df_candles['EMA144'] = df_candles['Close'].ewm(span=144, adjust=False).mean()
    df_candles['EMA169'] = df_candles['Close'].ewm(span=169, adjust=False).mean()
    df_candles['EMA576'] = df_candles['Close'].ewm(span=576, adjust=False).mean()
    df_candles['EMA676'] = df_candles['Close'].ewm(span=676, adjust=False).mean()

    # --- Equity & Markers ---
    initial_balance = 10000.0
    df_trades['profit_loss'] = pd.to_numeric(df_trades['profit_loss'])
    equity_series = pd.Series(index=df_candles.index, dtype=float)
    equity_series.iloc[0] = initial_balance
    current_balance = initial_balance
    
    buy_markers = [np.nan] * len(df_candles)
    sell_markers = [np.nan] * len(df_candles)
    
    for _, trade in df_trades.iterrows():
        entry_time = pd.to_datetime(trade['open_position_time'])
        exit_time = pd.to_datetime(trade['close_position_time']) if pd.notna(trade['close_position_time']) else None
        
        # Markers
        try:
            # Use nearest for robust matching
            idx_list = df_candles.index.get_indexer([entry_time], method='nearest')
            if len(idx_list) > 0 and idx_list[0] != -1:
                idx = idx_list[0]
                # High/Low placement
                if trade['option_type'] == 'long':
                    buy_markers[idx] = df_candles.iloc[idx]['Low'] * 0.99
                else:
                    sell_markers[idx] = df_candles.iloc[idx]['High'] * 1.01
        except: pass

        # Equity
        if exit_time and pd.notna(trade['profit_loss']):
            current_balance += float(trade['profit_loss'])
            try:
                idx_list = df_candles.index.get_indexer([exit_time], method='nearest')
                if len(idx_list) > 0 and idx_list[0] != -1:
                    equity_series.iloc[idx_list[0]] = current_balance
            except: pass
            
    equity_series = equity_series.ffill().bfill()

    apds = [
        mpf.make_addplot(df_candles['EMA144'], color='#2ca02c', width=1.5),
        mpf.make_addplot(df_candles['EMA169'], color='#2ca02c', width=1.5),
        mpf.make_addplot(df_candles['EMA576'], color='#1f77b4', width=2.0),
        mpf.make_addplot(df_candles['EMA676'], color='#1f77b4', width=2.0),
        mpf.make_addplot(buy_markers, type='scatter', markersize=50, marker='^', color='g'),
        mpf.make_addplot(sell_markers, type='scatter', markersize=50, marker='v', color='r'),
        # Panel 1 (Main is 0), Equity
        mpf.make_addplot(equity_series, panel=1, color='purple', ylabel='Equity', width=2)
    ]
    
    mc = mpf.make_marketcolors(up='#2ebd85', down='#f6465d', inherit=True)
    s  = mpf.make_mpf_style(base_mpf_style='nightclouds', marketcolors=mc)
    
    output_dir = "dist"
    if not os.path.exists(output_dir):
        os.makedirs(output_dir)
    file_path = f"{output_dir}/vegas_backtest_detail_{back_test_id}.png"
    
    profit_str = f"{float(log['profit']):.2f}"
    title = f"Vegas Backtest #{back_test_id} [{log['inst_type']} {log['time']}] PnL:{profit_str}%"
    
    print(f"Generating candle chart to {file_path}...")
    mpf.plot(
        df_candles,
        type='candle',
        style=s,
        addplot=apds,
        volume=False, # Volume REMOVED per request
        title=title,
        savefig=dict(fname=file_path, dpi=150, bbox_inches='tight'),
        warn_too_much_data=100000,
        panel_ratios=(3, 1), # Main, Equity
        figsize=(12, 8)
    )
    print("Done.")

def main(bid):
    df_candles, df_trades, log = fetch_data(bid)
    if df_candles is None or df_candles.empty:
        return
        
    # Chart 1: Candles + Equity (No Vol)
    plot_candle_chart(df_candles, df_trades, log, bid)
    
    # Chart 2: Stats
    stats = parse_signals(df_trades)
    plot_stats_chart(bid, stats)

if __name__ == "__main__":
    if len(sys.argv) > 1:
        bid = sys.argv[1]
    else:
        bid = 15650
    main(bid)
