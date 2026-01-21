-- Add signal_value column to filtered_signal_log table
-- Note: Run only if column doesn't exist. MySQL will error if column already exists.
ALTER TABLE filtered_signal_log ADD COLUMN signal_value JSON AFTER trade_result;
