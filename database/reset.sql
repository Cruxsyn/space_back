-- =============================================================================
-- Ship Game Database Reset Script
-- WARNING: This will DROP all tables and data!
-- Use only for development/testing
-- =============================================================================

-- Drop triggers first
DROP TRIGGER IF EXISTS on_auth_user_created ON auth.users;
DROP TRIGGER IF EXISTS update_items_updated_at ON items;
DROP TRIGGER IF EXISTS update_purchases_updated_at ON purchases;
DROP TRIGGER IF EXISTS update_player_stats_updated_at ON player_stats_aggregate;
DROP TRIGGER IF EXISTS on_purchase_paid ON purchases;

-- Drop functions
DROP FUNCTION IF EXISTS handle_new_user();
DROP FUNCTION IF EXISTS update_updated_at();
DROP FUNCTION IF EXISTS grant_item_on_purchase();

-- Drop views
DROP VIEW IF EXISTS user_inventory_details;
DROP VIEW IF EXISTS leaderboard_wins;
DROP VIEW IF EXISTS leaderboard_kd;

-- Drop tables (in dependency order)
DROP TABLE IF EXISTS player_match_stats CASCADE;
DROP TABLE IF EXISTS match_history CASCADE;
DROP TABLE IF EXISTS purchases CASCADE;
DROP TABLE IF EXISTS user_inventory CASCADE;
DROP TABLE IF EXISTS items CASCADE;
DROP TABLE IF EXISTS player_stats_aggregate CASCADE;
DROP TABLE IF EXISTS profiles CASCADE;

-- Confirmation message
DO $$
BEGIN
    RAISE NOTICE 'Database reset complete. All ship_game tables dropped.';
END $$;
