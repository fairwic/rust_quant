-- Add signal_value column to filtered_signal_log table if not exists
-- Use procedure to check column existence before adding
SET @dbname = DATABASE();
SET @tablename = 'filtered_signal_log';
SET @columnname = 'signal_value';
SET @preparedStatement = (SELECT IF(
  (
    SELECT COUNT(*) FROM INFORMATION_SCHEMA.COLUMNS
    WHERE 
      (TABLE_SCHEMA = @dbname)
      AND (TABLE_NAME = @tablename)
      AND (COLUMN_NAME = @columnname)
  ) > 0,
  'SELECT 1',
  'ALTER TABLE filtered_signal_log ADD COLUMN signal_value JSON AFTER trade_result'
));
PREPARE alterIfNotExists FROM @preparedStatement;
EXECUTE alterIfNotExists;
DEALLOCATE PREPARE alterIfNotExists;
