# Ship Game Database

This directory contains SQL scripts for setting up the Supabase PostgreSQL database.

## Files

| File | Description |
|------|-------------|
| `schema.sql` | Complete database schema with tables, indexes, RLS policies, functions, and sample data |
| `rls_policies.sql` | Standalone RLS policies (for auditing/reference) |
| `reset.sql` | Drops all tables - **USE WITH CAUTION** |

## Setup

### 1. Create Supabase Project

1. Go to [supabase.com](https://supabase.com) and create a new project
2. Note your project URL and keys from Settings > API

### 2. Run Schema

In the Supabase Dashboard SQL Editor, run the contents of `schema.sql`.

Or using the Supabase CLI:

```bash
supabase db push
```

### 3. Configure Environment

Update your server's `.env` file with:

```env
SUPABASE_URL=https://your-project.supabase.co
SUPABASE_SERVICE_ROLE_KEY=your-service-role-key
SUPABASE_ANON_KEY=your-anon-key
```

## Tables

### Core Tables

| Table | Description |
|-------|-------------|
| `profiles` | User profiles (display names, etc.) |
| `items` | Store items (flag skins, trail effects, etc.) |
| `user_inventory` | User's owned/equipped items |
| `purchases` | Stripe purchase records |

### Stats Tables (Optional)

| Table | Description |
|-------|-------------|
| `match_history` | Completed match records |
| `player_match_stats` | Per-player stats for each match |
| `player_stats_aggregate` | Lifetime aggregated player stats |

## Row Level Security (RLS)

All tables have RLS enabled. The policies follow these principles:

### User-Owned Data
- **profiles**: Users can only read/update their own profile
- **user_inventory**: Users can read their own inventory, update (equip/unequip)
- **purchases**: Users can only view their own purchase history

### Public Data
- **items**: All authenticated users can view active store items
- **match_history**: Public (for viewing past matches)
- **player_match_stats**: Public (for leaderboards)
- **player_stats_aggregate**: Public (for leaderboards)

### Service Role Operations

The server uses `service_role` key which **bypasses RLS**. This is used for:
- Creating purchase records
- Granting items after successful payment (via webhook)
- Recording match results and stats

## Triggers

| Trigger | Description |
|---------|-------------|
| `on_auth_user_created` | Auto-creates profile when user signs up |
| `on_purchase_paid` | Auto-grants item when purchase status becomes 'paid' |
| `update_*_updated_at` | Auto-updates `updated_at` timestamps |

## Views

| View | Description |
|------|-------------|
| `user_inventory_details` | Inventory with joined item details |
| `leaderboard_wins` | Top 100 players by wins |
| `leaderboard_kd` | Top 100 players by K/D ratio |

## Development

### Reset Database

⚠️ **WARNING**: This will delete ALL data!

```sql
-- In Supabase SQL Editor
\i reset.sql
\i schema.sql
```

### Test RLS Policies

```sql
-- Check which tables have RLS enabled
SELECT tablename, rowsecurity 
FROM pg_tables 
WHERE schemaname = 'public';

-- Test as specific user
SET request.jwt.claims = '{"sub": "user-uuid-here"}';
SELECT * FROM profiles;  -- Should only return that user's profile
```

## Stripe Integration

The `purchases` table integrates with Stripe:

1. **Checkout**: Server creates pending purchase with `stripe_session_id`
2. **Webhook**: On `checkout.session.completed`, status → 'paid'
3. **Trigger**: `on_purchase_paid` automatically grants item to user

Purchase statuses:
- `pending` - Checkout initiated
- `paid` - Payment successful
- `failed` - Payment failed
- `refunded` - Payment refunded (manual process)
