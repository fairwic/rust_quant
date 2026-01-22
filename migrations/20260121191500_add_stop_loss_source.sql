-- Add stop_loss_source column to back_test_detail table if not exists
-- Use procedure to check column existence before adding
SET @dbname = DATABASE();
SET @tablename = 'back_test_detail';
SET @columnname = 'stop_loss_source';
SET @preparedStatement = (SELECT IF(
  (
    SELECT COUNT(*) FROM INFORMATION_SCHEMA.COLUMNS
    WHERE 
      (TABLE_SCHEMA = @dbname)
      AND (TABLE_NAME = @tablename)
      AND (COLUMN_NAME = @columnname)
  ) > 0,
  'SELECT 1',
  'ALTER TABLE back_test_detail ADD COLUMN stop_loss_source VARCHAR(50) NULL AFTER signal_result'
));
PREPARE alterIfNotExists FROM @preparedStatement;
EXECUTE alterIfNotExists;
DEALLOCATE PREPARE alterIfNotExists;
