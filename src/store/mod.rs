//! Data store modules for Supabase integration

pub mod inventory;
pub mod profiles;
pub mod supabase;

pub use inventory::InventoryStore;
pub use profiles::ProfileStore;
pub use supabase::SupabaseClient;
