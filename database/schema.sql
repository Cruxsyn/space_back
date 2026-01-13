-- =============================================================================
-- Ship Game Database Schema
-- Supabase PostgreSQL
-- =============================================================================

-- Enable UUID extension (usually already enabled in Supabase)
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

-- =============================================================================
-- PROFILES TABLE
-- =============================================================================
-- Stores user profile information linked to Supabase Auth users

CREATE TABLE IF NOT EXISTS profiles (
    id UUID PRIMARY KEY REFERENCES auth.users(id) ON DELETE CASCADE,
    display_name TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Create index for faster lookups
CREATE INDEX IF NOT EXISTS idx_profiles_created_at ON profiles(created_at);

-- Enable RLS
ALTER TABLE profiles ENABLE ROW LEVEL SECURITY;

-- RLS Policies for profiles
-- Users can read their own profile
CREATE POLICY "Users can view own profile"
    ON profiles
    FOR SELECT
    USING (auth.uid() = id);

-- Users can insert their own profile
CREATE POLICY "Users can insert own profile"
    ON profiles
    FOR INSERT
    WITH CHECK (auth.uid() = id);

-- Users can update their own profile
CREATE POLICY "Users can update own profile"
    ON profiles
    FOR UPDATE
    USING (auth.uid() = id)
    WITH CHECK (auth.uid() = id);

-- =============================================================================
-- ITEMS TABLE
-- =============================================================================
-- Store items available for purchase (flag skins, ship cosmetics, etc.)

CREATE TABLE IF NOT EXISTS items (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    type TEXT NOT NULL,  -- e.g., 'flag_skin', 'ship_skin', 'trail_effect'
    name TEXT NOT NULL,
    description TEXT,
    price_usd INTEGER NOT NULL,  -- Price in cents (e.g., 499 = $4.99)
    stripe_price_id TEXT,  -- Optional: pre-created Stripe price ID
    preview_url TEXT,  -- URL to preview image
    rarity TEXT DEFAULT 'common',  -- common, rare, epic, legendary
    active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Create indexes
CREATE INDEX IF NOT EXISTS idx_items_type ON items(type);
CREATE INDEX IF NOT EXISTS idx_items_active ON items(active);
CREATE INDEX IF NOT EXISTS idx_items_type_active ON items(type, active);

-- Enable RLS
ALTER TABLE items ENABLE ROW LEVEL SECURITY;

-- RLS Policies for items
-- Anyone (authenticated) can view active items
CREATE POLICY "Anyone can view active items"
    ON items
    FOR SELECT
    USING (active = TRUE);

-- Only service role can insert/update/delete items (handled by bypassing RLS)

-- =============================================================================
-- USER_INVENTORY TABLE
-- =============================================================================
-- Tracks which items users own and have equipped

CREATE TABLE IF NOT EXISTS user_inventory (
    user_id UUID NOT NULL REFERENCES auth.users(id) ON DELETE CASCADE,
    item_id UUID NOT NULL REFERENCES items(id) ON DELETE CASCADE,
    owned BOOLEAN NOT NULL DEFAULT FALSE,
    equipped BOOLEAN NOT NULL DEFAULT FALSE,
    acquired_at TIMESTAMPTZ,  -- When the item was purchased/granted
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    
    PRIMARY KEY (user_id, item_id)
);

-- Create indexes for common queries
CREATE INDEX IF NOT EXISTS idx_user_inventory_user_id ON user_inventory(user_id);
CREATE INDEX IF NOT EXISTS idx_user_inventory_owned ON user_inventory(user_id, owned) WHERE owned = TRUE;
CREATE INDEX IF NOT EXISTS idx_user_inventory_equipped ON user_inventory(user_id, equipped) WHERE equipped = TRUE;

-- Enable RLS
ALTER TABLE user_inventory ENABLE ROW LEVEL SECURITY;

-- RLS Policies for user_inventory
-- Users can view their own inventory
CREATE POLICY "Users can view own inventory"
    ON user_inventory
    FOR SELECT
    USING (auth.uid() = user_id);

-- Users can update their own inventory (for equipping/unequipping)
CREATE POLICY "Users can update own inventory"
    ON user_inventory
    FOR UPDATE
    USING (auth.uid() = user_id)
    WITH CHECK (auth.uid() = user_id);

-- Only service role can insert inventory entries (via webhook after purchase)

-- =============================================================================
-- PURCHASES TABLE
-- =============================================================================
-- Records of all purchase transactions with Stripe

CREATE TABLE IF NOT EXISTS purchases (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id UUID NOT NULL REFERENCES auth.users(id) ON DELETE CASCADE,
    item_id UUID NOT NULL REFERENCES items(id) ON DELETE RESTRICT,
    stripe_session_id TEXT,
    stripe_payment_intent TEXT,
    status TEXT NOT NULL DEFAULT 'pending',  -- pending, paid, failed, refunded
    amount_usd INTEGER,  -- Amount in cents at time of purchase
    currency TEXT DEFAULT 'usd',
    error_message TEXT,  -- Store any error messages from failed payments
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Create indexes
CREATE INDEX IF NOT EXISTS idx_purchases_user_id ON purchases(user_id);
CREATE INDEX IF NOT EXISTS idx_purchases_status ON purchases(status);
CREATE INDEX IF NOT EXISTS idx_purchases_stripe_session ON purchases(stripe_session_id);
CREATE INDEX IF NOT EXISTS idx_purchases_stripe_intent ON purchases(stripe_payment_intent);
CREATE INDEX IF NOT EXISTS idx_purchases_user_status ON purchases(user_id, status);

-- Enable RLS
ALTER TABLE purchases ENABLE ROW LEVEL SECURITY;

-- RLS Policies for purchases
-- Users can view their own purchases
CREATE POLICY "Users can view own purchases"
    ON purchases
    FOR SELECT
    USING (auth.uid() = user_id);

-- Only service role can insert/update purchases (via server-side operations)

-- =============================================================================
-- MATCH_HISTORY TABLE (OPTIONAL - for stats tracking)
-- =============================================================================
-- Records completed matches for stats and leaderboards

CREATE TABLE IF NOT EXISTS match_history (
    id UUID PRIMARY KEY,  -- Match ID from server
    seed BIGINT NOT NULL,  -- Random seed used for the match
    started_at TIMESTAMPTZ NOT NULL,
    ended_at TIMESTAMPTZ NOT NULL,
    duration_secs INTEGER NOT NULL,
    total_players INTEGER NOT NULL,
    winner_user_id UUID REFERENCES auth.users(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Create index for leaderboard queries
CREATE INDEX IF NOT EXISTS idx_match_history_ended_at ON match_history(ended_at DESC);
CREATE INDEX IF NOT EXISTS idx_match_history_winner ON match_history(winner_user_id);

-- Enable RLS
ALTER TABLE match_history ENABLE ROW LEVEL SECURITY;

-- RLS Policies for match_history
-- Anyone can view match history
CREATE POLICY "Anyone can view match history"
    ON match_history
    FOR SELECT
    USING (TRUE);

-- =============================================================================
-- PLAYER_MATCH_STATS TABLE (OPTIONAL - for detailed stats)
-- =============================================================================
-- Per-player statistics for each match

CREATE TABLE IF NOT EXISTS player_match_stats (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    match_id UUID NOT NULL REFERENCES match_history(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES auth.users(id) ON DELETE CASCADE,
    ship_type TEXT NOT NULL,
    kills INTEGER NOT NULL DEFAULT 0,
    damage_dealt REAL NOT NULL DEFAULT 0,
    damage_taken REAL NOT NULL DEFAULT 0,
    shots_fired INTEGER NOT NULL DEFAULT 0,
    shots_hit INTEGER NOT NULL DEFAULT 0,
    accuracy REAL GENERATED ALWAYS AS (
        CASE WHEN shots_fired > 0 THEN shots_hit::REAL / shots_fired ELSE 0 END
    ) STORED,
    placement INTEGER NOT NULL,  -- Final placement (1 = winner)
    alive_time_secs INTEGER NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    
    UNIQUE (match_id, user_id)
);

-- Create indexes
CREATE INDEX IF NOT EXISTS idx_player_match_stats_user ON player_match_stats(user_id);
CREATE INDEX IF NOT EXISTS idx_player_match_stats_match ON player_match_stats(match_id);
CREATE INDEX IF NOT EXISTS idx_player_match_stats_placement ON player_match_stats(placement);
CREATE INDEX IF NOT EXISTS idx_player_match_stats_kills ON player_match_stats(kills DESC);

-- Enable RLS
ALTER TABLE player_match_stats ENABLE ROW LEVEL SECURITY;

-- RLS Policies for player_match_stats
-- Anyone can view player stats
CREATE POLICY "Anyone can view player match stats"
    ON player_match_stats
    FOR SELECT
    USING (TRUE);

-- =============================================================================
-- PLAYER_STATS_AGGREGATE TABLE (OPTIONAL - for lifetime stats)
-- =============================================================================
-- Aggregated lifetime statistics per player

CREATE TABLE IF NOT EXISTS player_stats_aggregate (
    user_id UUID PRIMARY KEY REFERENCES auth.users(id) ON DELETE CASCADE,
    total_matches INTEGER NOT NULL DEFAULT 0,
    total_wins INTEGER NOT NULL DEFAULT 0,
    total_kills INTEGER NOT NULL DEFAULT 0,
    total_deaths INTEGER NOT NULL DEFAULT 0,
    total_damage_dealt REAL NOT NULL DEFAULT 0,
    total_damage_taken REAL NOT NULL DEFAULT 0,
    total_shots_fired INTEGER NOT NULL DEFAULT 0,
    total_shots_hit INTEGER NOT NULL DEFAULT 0,
    best_placement INTEGER,
    total_playtime_secs INTEGER NOT NULL DEFAULT 0,
    win_rate REAL GENERATED ALWAYS AS (
        CASE WHEN total_matches > 0 THEN total_wins::REAL / total_matches ELSE 0 END
    ) STORED,
    kd_ratio REAL GENERATED ALWAYS AS (
        CASE WHEN total_deaths > 0 THEN total_kills::REAL / total_deaths ELSE total_kills::REAL END
    ) STORED,
    overall_accuracy REAL GENERATED ALWAYS AS (
        CASE WHEN total_shots_fired > 0 THEN total_shots_hit::REAL / total_shots_fired ELSE 0 END
    ) STORED,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Create indexes for leaderboards
CREATE INDEX IF NOT EXISTS idx_player_stats_wins ON player_stats_aggregate(total_wins DESC);
CREATE INDEX IF NOT EXISTS idx_player_stats_kills ON player_stats_aggregate(total_kills DESC);
CREATE INDEX IF NOT EXISTS idx_player_stats_kd ON player_stats_aggregate(kd_ratio DESC);
CREATE INDEX IF NOT EXISTS idx_player_stats_win_rate ON player_stats_aggregate(win_rate DESC) 
    WHERE total_matches >= 10;  -- Only include players with enough matches

-- Enable RLS
ALTER TABLE player_stats_aggregate ENABLE ROW LEVEL SECURITY;

-- RLS Policies for player_stats_aggregate
-- Anyone can view aggregate stats
CREATE POLICY "Anyone can view player aggregate stats"
    ON player_stats_aggregate
    FOR SELECT
    USING (TRUE);

-- Users can view their own detailed stats
CREATE POLICY "Users can view own detailed stats"
    ON player_stats_aggregate
    FOR SELECT
    USING (auth.uid() = user_id);

-- =============================================================================
-- FUNCTIONS & TRIGGERS
-- =============================================================================

-- Function to automatically create a profile when a new user signs up
CREATE OR REPLACE FUNCTION handle_new_user()
RETURNS TRIGGER AS $$
BEGIN
    INSERT INTO profiles (id, display_name)
    VALUES (NEW.id, COALESCE(NEW.raw_user_meta_data->>'display_name', 'Player_' || LEFT(NEW.id::TEXT, 8)));
    
    -- Also initialize aggregate stats
    INSERT INTO player_stats_aggregate (user_id)
    VALUES (NEW.id);
    
    RETURN NEW;
END;
$$ LANGUAGE plpgsql SECURITY DEFINER;

-- Trigger to create profile on user signup
DROP TRIGGER IF EXISTS on_auth_user_created ON auth.users;
CREATE TRIGGER on_auth_user_created
    AFTER INSERT ON auth.users
    FOR EACH ROW
    EXECUTE FUNCTION handle_new_user();

-- Function to update timestamps on modification
CREATE OR REPLACE FUNCTION update_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Apply updated_at trigger to relevant tables
DROP TRIGGER IF EXISTS update_items_updated_at ON items;
CREATE TRIGGER update_items_updated_at
    BEFORE UPDATE ON items
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at();

DROP TRIGGER IF EXISTS update_purchases_updated_at ON purchases;
CREATE TRIGGER update_purchases_updated_at
    BEFORE UPDATE ON purchases
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at();

DROP TRIGGER IF EXISTS update_player_stats_updated_at ON player_stats_aggregate;
CREATE TRIGGER update_player_stats_updated_at
    BEFORE UPDATE ON player_stats_aggregate
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at();

-- Function to grant item to user after successful purchase
CREATE OR REPLACE FUNCTION grant_item_on_purchase()
RETURNS TRIGGER AS $$
BEGIN
    -- Only process when status changes to 'paid'
    IF NEW.status = 'paid' AND (OLD.status IS NULL OR OLD.status != 'paid') THEN
        INSERT INTO user_inventory (user_id, item_id, owned, equipped, acquired_at)
        VALUES (NEW.user_id, NEW.item_id, TRUE, FALSE, NOW())
        ON CONFLICT (user_id, item_id) 
        DO UPDATE SET owned = TRUE, acquired_at = COALESCE(user_inventory.acquired_at, NOW());
    END IF;
    
    RETURN NEW;
END;
$$ LANGUAGE plpgsql SECURITY DEFINER;

-- Trigger to auto-grant items on successful purchase
DROP TRIGGER IF EXISTS on_purchase_paid ON purchases;
CREATE TRIGGER on_purchase_paid
    AFTER INSERT OR UPDATE ON purchases
    FOR EACH ROW
    EXECUTE FUNCTION grant_item_on_purchase();

-- =============================================================================
-- SAMPLE DATA (for testing)
-- =============================================================================

-- Insert some sample items
INSERT INTO items (id, type, name, description, price_usd, rarity, active) VALUES
    (uuid_generate_v4(), 'flag_skin', 'Jolly Roger', 'Classic pirate flag with skull and crossbones', 299, 'common', TRUE),
    (uuid_generate_v4(), 'flag_skin', 'Royal Navy', 'Distinguished naval ensign', 499, 'rare', TRUE),
    (uuid_generate_v4(), 'flag_skin', 'Kraken''s Mark', 'Feared flag of the deep sea terror', 799, 'epic', TRUE),
    (uuid_generate_v4(), 'flag_skin', 'Golden Phoenix', 'Legendary flag that burns with eternal flame', 1499, 'legendary', TRUE),
    (uuid_generate_v4(), 'trail_effect', 'Sea Foam', 'Enhanced wake trail with foam effects', 399, 'common', TRUE),
    (uuid_generate_v4(), 'trail_effect', 'Bioluminescence', 'Glowing trail of ocean life', 699, 'rare', TRUE)
ON CONFLICT DO NOTHING;

-- =============================================================================
-- VIEWS (for convenience)
-- =============================================================================

-- View for user inventory with item details
CREATE OR REPLACE VIEW user_inventory_details AS
SELECT 
    ui.user_id,
    ui.item_id,
    ui.owned,
    ui.equipped,
    ui.acquired_at,
    i.type AS item_type,
    i.name AS item_name,
    i.description,
    i.rarity,
    i.preview_url
FROM user_inventory ui
JOIN items i ON ui.item_id = i.id
WHERE ui.owned = TRUE;

-- View for leaderboard (top players by wins)
CREATE OR REPLACE VIEW leaderboard_wins AS
SELECT 
    p.id AS user_id,
    p.display_name,
    COALESCE(s.total_matches, 0) AS total_matches,
    COALESCE(s.total_wins, 0) AS total_wins,
    COALESCE(s.total_kills, 0) AS total_kills,
    COALESCE(s.win_rate, 0) AS win_rate,
    COALESCE(s.kd_ratio, 0) AS kd_ratio
FROM profiles p
LEFT JOIN player_stats_aggregate s ON p.id = s.user_id
WHERE COALESCE(s.total_matches, 0) >= 5  -- Minimum matches to appear
ORDER BY total_wins DESC, win_rate DESC
LIMIT 100;

-- View for leaderboard (top players by K/D)
CREATE OR REPLACE VIEW leaderboard_kd AS
SELECT 
    p.id AS user_id,
    p.display_name,
    COALESCE(s.total_matches, 0) AS total_matches,
    COALESCE(s.total_kills, 0) AS total_kills,
    COALESCE(s.total_deaths, 0) AS total_deaths,
    COALESCE(s.kd_ratio, 0) AS kd_ratio,
    COALESCE(s.overall_accuracy, 0) AS accuracy
FROM profiles p
LEFT JOIN player_stats_aggregate s ON p.id = s.user_id
WHERE COALESCE(s.total_matches, 0) >= 10  -- Higher threshold for K/D
ORDER BY kd_ratio DESC, total_kills DESC
LIMIT 100;
