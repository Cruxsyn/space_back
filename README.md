# Ship Game Server

Authoritative multiplayer game server for a ship battle royale game.

## Architecture

This server is the **source of truth** for all game state:
- Matchmaking and match lifecycle
- Player state (position, velocity, health, alive status)
- Damage, weapons, zone shrink/damage
- Tick simulation + snapshot broadcasting
- Authentication (Supabase JWT validation)
- Inventory unlocks (flag skins) + equip state
- Stripe payments → webhook → grant item

## Tech Stack

- **Rust** (stable)
- **Tokio** async runtime
- **Axum** for HTTP + WebSocket
- **Supabase** for user data and auth
- **Stripe** for payments

## Project Structure

```
server/
├── Cargo.toml
├── src/
│   ├── main.rs              # Entry point
│   ├── app/                 # Application state
│   │   └── state.rs
│   ├── config/              # Environment parsing
│   │   └── mod.rs
│   ├── http/                # HTTP routes & middleware
│   │   ├── middleware.rs    # JWT auth
│   │   └── routes.rs
│   ├── ws/                  # WebSocket handling
│   │   ├── handler.rs       # WS upgrade + session
│   │   └── protocol.rs      # ClientMsg/ServerMsg types
│   ├── matchmaking/         # Player queue & service
│   │   ├── queue.rs
│   │   └── service.rs
│   ├── game/                # Core game simulation
│   │   ├── match.rs         # Match state & tick loop
│   │   ├── physics.rs       # Ship movement
│   │   ├── combat.rs        # Weapons & damage
│   │   └── snapshot.rs      # Network snapshots
│   ├── store/               # Data access
│   │   ├── supabase.rs      # Supabase REST client
│   │   ├── inventory.rs
│   │   └── profiles.rs
│   ├── payments/            # Stripe integration
│   │   ├── stripe.rs        # Checkout sessions
│   │   └── webhook.rs       # Webhook handler
│   └── util/                # Utilities
│       ├── time.rs
│       └── rate_limit.rs
```

## Environment Variables

Create a `.env` file based on `.env.example`:

```env
# Server
SERVER_ADDR=0.0.0.0:8080
LOG_LEVEL=info

# Supabase
SUPABASE_URL=https://xxxxx.supabase.co
SUPABASE_ANON_KEY=...
SUPABASE_SERVICE_ROLE_KEY=...
SUPABASE_JWT_SECRET=...

# Stripe
STRIPE_SECRET_KEY=sk_live_...
STRIPE_WEBHOOK_SECRET=whsec_...

# URLs
PUBLIC_BASE_URL=https://yourdomain.com
CLIENT_ORIGIN=https://yourgame.pages.dev
```

⚠️ **Security**: The `SUPABASE_SERVICE_ROLE_KEY` bypasses RLS. Treat it like a production root password.

## API Endpoints

### Public (no auth)

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/health` | Server health check |
| GET | `/ws?token=...` | WebSocket connection |
| POST | `/payments/webhook` | Stripe webhook |

### Protected (requires Bearer token)

| Method | Endpoint | Description |
|--------|----------|-------------|
| POST | `/matchmaking/join` | Join matchmaking queue |
| POST | `/payments/checkout` | Create Stripe checkout session |
| GET | `/inventory` | Get user inventory |
| POST | `/inventory/equip` | Equip an item |

## WebSocket Protocol

### Client → Server Messages

```json
// Join a match
{"type": "join_match", "match_id": null, "ship_type": "fighter"}

// Send input each tick
{"type": "input_tick", "seq": 1, "throttle": 0.5, "steer": -0.3, "shoot": true, "aim_yaw": 1.57}

// Ping for latency
{"type": "ping", "t": 1234567890}

// Leave match
{"type": "leave_match"}
```

### Server → Client Messages

```json
// Welcome on connect
{"type": "welcome", "user_id": "...", "server_time": 1234567890}

// Match joined confirmation
{"type": "match_joined", "match_id": "...", "seed": 12345, "players": [...]}

// Game state snapshot (sent at ~20 TPS)
{"type": "snapshot", "tick": 100, "zone": {...}, "players": [...], "events": [...]}

// Match ended
{"type": "match_end", "winner_user_id": "...", "stats": {...}}
```

## Game Mechanics

### Ship Types

| Type | Speed | Health | Turn Rate | Damage |
|------|-------|--------|-----------|--------|
| Scout | Fast | Low | High | Low |
| Fighter | Medium | Medium | Medium | Medium |
| Cruiser | Slow | High | Low | Medium |
| Destroyer | Slowest | Medium | Lowest | High |

### Battle Royale Zone

The play area shrinks over time:
1. Initial radius: 1500 units
2. Phase 1: Shrink to 1000 (60s delay, 30s shrink)
3. Phase 2: Shrink to 600
4. Phase 3: Shrink to 300
5. Phase 4: Shrink to 50 (final)

Players outside the zone take damage per second.

### Tick Rates

- Simulation: 30 TPS
- Network snapshots: 20 TPS

## Running

```bash
# Development
cargo run

# Release build
cargo build --release
./target/release/ship_game_server
```

## Database Schema (Supabase)

Required tables:

```sql
-- User profiles
CREATE TABLE profiles (
  id UUID PRIMARY KEY REFERENCES auth.users(id),
  display_name TEXT,
  created_at TIMESTAMPTZ DEFAULT NOW()
);

-- Store items
CREATE TABLE items (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  type TEXT NOT NULL,
  name TEXT NOT NULL,
  price_usd INTEGER NOT NULL,
  stripe_price_id TEXT,
  active BOOLEAN DEFAULT true
);

-- User inventory
CREATE TABLE user_inventory (
  user_id UUID REFERENCES auth.users(id),
  item_id UUID REFERENCES items(id),
  owned BOOLEAN DEFAULT false,
  equipped BOOLEAN DEFAULT false,
  created_at TIMESTAMPTZ DEFAULT NOW(),
  PRIMARY KEY (user_id, item_id)
);

-- Purchases
CREATE TABLE purchases (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  user_id UUID REFERENCES auth.users(id),
  stripe_session_id TEXT,
  stripe_payment_intent TEXT,
  item_id UUID REFERENCES items(id),
  status TEXT DEFAULT 'pending',
  created_at TIMESTAMPTZ DEFAULT NOW()
);
```

## Security Considerations

1. **JWT Verification**: All protected endpoints verify Supabase JWTs
2. **WebSocket Auth**: Token required in query string for WS upgrade
3. **Stripe Webhooks**: HMAC signature verification required
4. **Rate Limiting**: Applied to inputs and API endpoints
5. **Server Authority**: Client inputs are validated; server never trusts client state
