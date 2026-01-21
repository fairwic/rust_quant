-- Migration: Add stop_loss_source column to back_test_detail
-- This column records the stop loss trigger source (e.g., "Engulfing", "KlineHammer")

ALTER TABLE back_test_detail ADD COLUMN stop_loss_source VARCHAR(50) NULL AFTER signal_result;
