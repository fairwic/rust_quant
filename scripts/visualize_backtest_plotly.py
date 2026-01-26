import pymysql
import pandas as pd
import plotly.graph_objects as go
from plotly.subplots import make_subplots
import sys
import os
import json
import numpy as np

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
        
        query_candles = f"SELECT ts, o, h, l, c, vol FROM `{table_name}` WHERE ts >= %s AND ts <= %s ORDER BY ts ASC"
        
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

def parse_stats(df_trades):
    stats = {} 
    for _, trade in df_trades.iterrows():
        try:
            pnl = float(trade['profit_loss'])
            is_win = pnl > 0
            sig_res_str = trade['signal_result']
            if not sig_res_str: continue
            
            sig_list = json.loads(sig_res_str)
            trade_type = trade['option_type'] 
            
            for item in sig_list:
                if len(item) < 2: continue
                sig_name = item[0]
                details = item[1]
                
                contributed = False
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
                
                # Logic to capture explicit signals or fallback to presence
                if contributed:
                    if sig_name not in stats: stats[sig_name] = {'win': 0, 'loss': 0, 'profit': 0.0}
                    if is_win:
                        stats[sig_name]['win'] += 1
                        stats[sig_name]['profit'] += pnl
                    else:
                        stats[sig_name]['loss'] += 1
                        stats[sig_name]['profit'] += pnl
        except: pass
    return stats

def plot_dashboard(back_test_id):
    df_candles, df_trades, log = fetch_data(back_test_id)
    if df_candles is None or df_candles.empty:
        print("No data.")
        return

    # --- Data Processing ---
    for col in ['o', 'h', 'l', 'c', 'vol']:
        df_candles[col] = pd.to_numeric(df_candles[col])
    df_candles['ts'] = pd.to_numeric(df_candles['ts'])
    df_candles['Date'] = pd.to_datetime(df_candles['ts'], unit='ms')
    
    # EMAs
    close = df_candles['c']
    df_candles['EMA144'] = close.ewm(span=144, adjust=False).mean()
    df_candles['EMA169'] = close.ewm(span=169, adjust=False).mean()
    df_candles['EMA576'] = close.ewm(span=576, adjust=False).mean()
    df_candles['EMA676'] = close.ewm(span=676, adjust=False).mean()

    # Equity Curve
    initial_balance = 10000.0
    df_trades['profit_loss'] = pd.to_numeric(df_trades['profit_loss'])
    
    # Map equity updates
    equity_data = []
    current_balance = initial_balance
    equity_data.append({'time': df_candles['Date'].iloc[0], 'equity': initial_balance})
    
    # Markers
    buy_markers_x = []
    buy_markers_y = []
    sell_markers_x = []
    sell_markers_y = []
    
    # Trades processing
    # To conform equity plot to candle timestamps, we can resample or just plot trade points
    # Let's plot trade exit points for equity
    
    sorted_trades = df_trades.sort_values('close_position_time') 
    
    # We will build a full equity Series aligned with Candles for proper zooming
    equity_series = pd.Series(index=df_candles['Date'], dtype=float)
    equity_series.iloc[0] = initial_balance
    
    trade_ptr = 0
    curr_bal = initial_balance
    
    # Create a quick lookup for equity change at each timestamp
    # Group by close time
    df_trades['close_ts'] = pd.to_datetime(df_trades['close_position_time'])
    pnl_by_time = df_trades.groupby('close_ts')['profit_loss'].sum()
    
    # Determine trade markers
    for _, trade in df_trades.iterrows():
        entry_ts = pd.to_datetime(trade['open_position_time'])
        
        # Approximate price for marker
        # Find closest candle row
        # (For simplicity we just use the timestamp on x-axis, plotly handles it)
        
        if trade['option_type'] == 'long':
            buy_markers_x.append(entry_ts)
            buy_markers_y.append(float(trade['open_price'])) # Or slightly below
        else:
            sell_markers_x.append(entry_ts)
            sell_markers_y.append(float(trade['open_price'])) # Or slightly above

    # Build Equity Curve aligned to Candle Index (Step-wise)
    # We iterate candles; if timestamp >= trade_close_time, update balance
    # This is O(N*M), slow. Better:
    # Use reindex/ffill on pnl_by_time
    
    pnl_aligned = pnl_by_time.reindex(df_candles['Date'], method='nearest', tolerance=pd.Timedelta('4h')) 
    # This might miss multiple trades in one candle or misalign.
    # Simpler: Just plotting Line Chart of (Time, Equity) using trade points is enough for Plotly.
    
    equity_x = [df_candles['Date'].iloc[0]]
    equity_y = [initial_balance]
    
    running_bal = initial_balance
    for t_time, pnl in pnl_by_time.items():
        running_bal += float(pnl)
        equity_x.append(t_time)
        equity_y.append(running_bal)
        
    # Stats
    stats = parse_stats(df_trades)
    stats_labels = sorted(stats.keys())
    stats_wins = [stats[k]['win'] for k in stats_labels]
    stats_losses = [stats[k]['loss'] for k in stats_labels]

    # --- Plotly Figure ---
    fig = make_subplots(
        rows=3, cols=1, 
        shared_xaxes=True, 
        vertical_spacing=0.05,
        row_heights=[0.6, 0.2, 0.2],
        specs=[[{"secondary_y": False}], [{"secondary_y": False}], [{"secondary_y": False}]],
        subplot_titles=("Price Action & Vegas Tunnel", "Account Equity", "Signal Performance")
    )

    # 1. Candlestick
    fig.add_trace(go.Candlestick(
        x=df_candles['Date'],
        open=df_candles['o'], high=df_candles['h'],
        low=df_candles['l'], close=df_candles['c'],
        name='OHLC'
    ), row=1, col=1)
    
    # EMAs
    fig.add_trace(go.Scatter(x=df_candles['Date'], y=df_candles['EMA144'], line=dict(color='#2ca02c', width=1), name='EMA 144'), row=1, col=1)
    fig.add_trace(go.Scatter(x=df_candles['Date'], y=df_candles['EMA169'], line=dict(color='#2ca02c', width=1), name='EMA 169'), row=1, col=1)
    fig.add_trace(go.Scatter(x=df_candles['Date'], y=df_candles['EMA576'], line=dict(color='#1f77b4', width=2), name='EMA 576'), row=1, col=1)
    fig.add_trace(go.Scatter(x=df_candles['Date'], y=df_candles['EMA676'], line=dict(color='#1f77b4', width=2), name='EMA 676'), row=1, col=1)

    # Markers
    fig.add_trace(go.Scatter(
        x=buy_markers_x, y=buy_markers_y, 
        mode='markers', marker=dict(symbol='triangle-up', size=10, color='#00ff00'),
        name='Long Entry'
    ), row=1, col=1)
    fig.add_trace(go.Scatter(
        x=sell_markers_x, y=sell_markers_y, 
        mode='markers', marker=dict(symbol='triangle-down', size=10, color='#ff0000'),
        name='Short Entry'
    ), row=1, col=1)

    # 2. Equity
    fig.add_trace(go.Scatter(
        x=equity_x, y=equity_y,
        mode='lines', line=dict(color='purple', width=2),
        name='Equity'
    ), row=2, col=1)

    # 3. Stats (Bar Chart)
    # Note: Bar chart x-axis is categorical, shared_xaxes might mess it up if axis type assumes date.
    # We should probably UNLINK x-axis for row 3 or use a secondary axis concept.
    # Actually make_subplots shared_xaxes=True forces row 3 to share range with row 1 (Time).
    # We MUST disable shared_xaxes for row 3. 
    # But make_subplots applies it column-wise.
    # Solution: We will print stats in a separate HTML div or just overlay on the chart?
    # Or, we can just use a separate figure in the same HTML?
    # Let's try to mix types. If shared_xaxes is True, it expects same type.
    # We will disable shared_xaxes for the whole figure and manually link Row 1 and 2.
    
    # Re-create figure without global shared_xaxes
    fig = make_subplots(
        rows=3, cols=1, 
        shared_xaxes=False, # Manual linking
        vertical_spacing=0.08,
        row_heights=[0.6, 0.2, 0.2],
        subplot_titles=("Price Action (Vegas)", "Equity Curve", "Signal Win/Loss Stats")
    )
    
    # Re-add traces
    # Row 1
    fig.add_trace(go.Candlestick(x=df_candles['Date'], open=df_candles['o'], high=df_candles['h'], low=df_candles['l'], close=df_candles['c'], name='OHLC'), row=1, col=1)
    fig.add_trace(go.Scatter(x=df_candles['Date'], y=df_candles['EMA144'], line=dict(color='green', width=1), name='EMA144'), row=1, col=1)
    fig.add_trace(go.Scatter(x=df_candles['Date'], y=df_candles['EMA169'], line=dict(color='green', width=1), name='EMA169'), row=1, col=1)
    fig.add_trace(go.Scatter(x=df_candles['Date'], y=df_candles['EMA576'], line=dict(color='blue', width=2), name='EMA576'), row=1, col=1)
    fig.add_trace(go.Scatter(x=df_candles['Date'], y=df_candles['EMA676'], line=dict(color='blue', width=2), name='EMA676'), row=1, col=1)
    fig.add_trace(go.Scatter(x=buy_markers_x, y=buy_markers_y, mode='markers', marker=dict(symbol='triangle-up', size=10, color='lime'), name='Long'), row=1, col=1)
    fig.add_trace(go.Scatter(x=sell_markers_x, y=sell_markers_y, mode='markers', marker=dict(symbol='triangle-down', size=10, color='red'), name='Short'), row=1, col=1)

    # Row 2
    fig.add_trace(go.Scatter(x=equity_x, y=equity_y, mode='lines', line=dict(color='violet', width=2), name='Equity'), row=2, col=1)

    # Row 3 (Categorical)
    fig.add_trace(go.Bar(x=stats_labels, y=stats_wins, name='Wins', marker_color='green'), row=3, col=1)
    fig.add_trace(go.Bar(x=stats_labels, y=stats_losses, name='Losses', marker_color='red'), row=3, col=1)

    # Layout Updates
    fig.update_layout(
        template='plotly_dark',
        height=1000,
        title_text=f"Backtest {back_test_id}: {log['inst_type']} {log['time']} (Profit: {float(log['profit']):.2f}%)",
        xaxis_rangeslider_visible=False
    )
    
    # Link X axes for Row 1 and 2 manually? Not easy in Python API without shared_xaxes=True.
    # Let's enable shared_xaxes for matches.
    # Actually, Plotly allows 'matches' parameter in layout.xaxis.
    fig.update_xaxes(matches='x', row=1, col=1)
    fig.update_xaxes(matches='x', row=2, col=1)
    # Row 3 is independent (Categorical) so no 'matches'.
    
    # Hide x-axis labels for Row 1
    fig.update_xaxes(showticklabels=False, row=1, col=1)

    output_dir = "dist"
    if not os.path.exists(output_dir): os.makedirs(output_dir)
    file_path = f"{output_dir}/vegas_backtest_{back_test_id}.html"
    
    print(f"Generating HTML dashboard to {file_path}...")
    fig.write_html(file_path)
    print("Done.")

if __name__ == "__main__":
    if len(sys.argv) > 1:
        bid = sys.argv[1]
    else:
        bid = 15650
    plot_dashboard(bid)
