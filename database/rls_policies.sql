-- =============================================================================
-- Ship Game Row Level Security (RLS) Policies
-- =============================================================================
-- This file contains only the RLS policies for reference/auditing.
-- These are also included in schema.sql
-- =============================================================================

-- =============================================================================
-- PROFILES RLS
-- =============================================================================

ALTER TABLE profiles ENABLE ROW LEVEL SECURITY;

-- Drop existing policies first (for idempotent runs)
DROP POLICY IF EXISTS "Users can view own profile" ON profiles;
DROP POLICY IF EXISTS "Users can insert own profile" ON profiles;
DROP POLICY IF EXISTS "Users can update own profile" ON profiles;
DROP POLICY IF EXISTS "Service role full access profiles" ON profiles;

-- Users can read their own profile
CREATE POLICY "Users can view own profile"
    ON profiles
    FOR SELECT
    USING (auth.uid() = id);

-- Users can insert their own profile (during signup)
CREATE POLICY "Users can insert own profile"
    ON profiles
    FOR INSERT
    WITH CHECK (auth.uid() = id);

-- Users can update their own profile (display name, etc.)
CREATE POLICY "Users can update own profile"
    ON profiles
    FOR UPDATE
    USING (auth.uid() = id)
    WITH CHECK (auth.uid() = id);

-- =============================================================================
-- ITEMS RLS
-- =============================================================================

ALTER TABLE items ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS "Anyone can view active items" ON items;
DROP POLICY IF EXISTS "Authenticated can view active items" ON items;

-- All authenticated users can view active items in the store
CREATE POLICY "Anyone can view active items"
    ON items
    FOR SELECT
    USING (active = TRUE);

-- Note: INSERT/UPDATE/DELETE are handled by service_role key (bypasses RLS)

-- =============================================================================
-- USER_INVENTORY RLS
-- =============================================================================

ALTER TABLE user_inventory ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS "Users can view own inventory" ON user_inventory;
DROP POLICY IF EXISTS "Users can update own inventory" ON user_inventory;

-- Users can view their own inventory
CREATE POLICY "Users can view own inventory"
    ON user_inventory
    FOR SELECT
    USING (auth.uid() = user_id);

-- Users can update their own inventory (equip/unequip items)
CREATE POLICY "Users can update own inventory"
    ON user_inventory
    FOR UPDATE
    USING (auth.uid() = user_id)
    WITH CHECK (auth.uid() = user_id);

-- Note: INSERT is handled by service_role via webhook after purchase

-- =============================================================================
-- PURCHASES RLS
-- =============================================================================

ALTER TABLE purchases ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS "Users can view own purchases" ON purchases;

-- Users can only view their own purchase history
CREATE POLICY "Users can view own purchases"
    ON purchases
    FOR SELECT
    USING (auth.uid() = user_id);

-- Note: INSERT/UPDATE handled by service_role via server/webhook

-- =============================================================================
-- MATCH_HISTORY RLS
-- =============================================================================

ALTER TABLE match_history ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS "Anyone can view match history" ON match_history;

-- Match history is public (for replay viewing, stats, etc.)
CREATE POLICY "Anyone can view match history"
    ON match_history
    FOR SELECT
    USING (TRUE);

-- =============================================================================
-- PLAYER_MATCH_STATS RLS
-- =============================================================================

ALTER TABLE player_match_stats ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS "Anyone can view player match stats" ON player_match_stats;

-- Player match stats are public (for leaderboards)
CREATE POLICY "Anyone can view player match stats"
    ON player_match_stats
    FOR SELECT
    USING (TRUE);

-- =============================================================================
-- PLAYER_STATS_AGGREGATE RLS
-- =============================================================================

ALTER TABLE player_stats_aggregate ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS "Anyone can view player aggregate stats" ON player_stats_aggregate;

-- Aggregate stats are public (for leaderboards)
CREATE POLICY "Anyone can view player aggregate stats"
    ON player_stats_aggregate
    FOR SELECT
    USING (TRUE);

-- =============================================================================
-- RLS VERIFICATION QUERY
-- =============================================================================
-- Run this to verify RLS is enabled on all tables:

-- SELECT 
--     schemaname,
--     tablename,
--     rowsecurity
-- FROM pg_tables 
-- WHERE schemaname = 'public' 
-- AND tablename IN (
--     'profiles', 
--     'items', 
--     'user_inventory', 
--     'purchases',
--     'match_history',
--     'player_match_stats',
--     'player_stats_aggregate'
-- );
