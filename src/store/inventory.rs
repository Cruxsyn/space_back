//! Inventory management - server-side only

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::supabase::{SupabaseClient, SupabaseError};

/// User inventory item
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInventoryItem {
    pub user_id: Uuid,
    pub item_id: Uuid,
    pub owned: bool,
    pub equipped: bool,
    pub created_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Inventory item with item details (joined)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InventoryItemWithDetails {
    pub item_id: Uuid,
    pub owned: bool,
    pub equipped: bool,
    #[serde(rename = "items")]
    pub item: Option<ItemDetails>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemDetails {
    pub id: Uuid,
    pub name: String,
    #[serde(rename = "type")]
    pub item_type: String,
}

/// New inventory entry for insertion
#[derive(Debug, Clone, Serialize)]
pub struct NewInventoryEntry {
    pub user_id: Uuid,
    pub item_id: Uuid,
    pub owned: bool,
    pub equipped: bool,
}

/// Inventory store operations
#[derive(Clone)]
pub struct InventoryStore {
    client: SupabaseClient,
}

impl InventoryStore {
    pub fn new(client: SupabaseClient) -> Self {
        Self { client }
    }

    /// Get all inventory items for a user
    pub async fn get_user_inventory(
        &self,
        user_id: Uuid,
    ) -> Result<Vec<UserInventoryItem>, SupabaseError> {
        let query = format!("user_id=eq.{}&owned=eq.true", user_id);
        self.client.get("user_inventory", &query).await
    }

    /// Get inventory with item details
    pub async fn get_user_inventory_with_details(
        &self,
        user_id: Uuid,
    ) -> Result<Vec<InventoryItemWithDetails>, SupabaseError> {
        let query = format!(
            "user_id=eq.{}&owned=eq.true&select=item_id,owned,equipped,items(id,name,type)",
            user_id
        );
        self.client.get("user_inventory", &query).await
    }

    /// Check if user owns a specific item
    pub async fn user_owns_item(
        &self,
        user_id: Uuid,
        item_id: Uuid,
    ) -> Result<bool, SupabaseError> {
        let query = format!(
            "user_id=eq.{}&item_id=eq.{}&owned=eq.true",
            user_id, item_id
        );
        let items: Vec<UserInventoryItem> = self.client.get("user_inventory", &query).await?;
        Ok(!items.is_empty())
    }

    /// Grant an item to a user (set owned = true)
    pub async fn grant_item(&self, user_id: Uuid, item_id: Uuid) -> Result<(), SupabaseError> {
        let entry = NewInventoryEntry {
            user_id,
            item_id,
            owned: true,
            equipped: false,
        };

        self.client
            .upsert("user_inventory", &entry, "user_id,item_id")
            .await
    }

    /// Equip an item (only one flag skin can be equipped at a time)
    pub async fn equip_item(&self, user_id: Uuid, item_id: Uuid) -> Result<(), SupabaseError> {
        // First, unequip all other flag skins for this user
        // We need to get the item type first
        let item_query = format!("id=eq.{}", item_id);
        let items: Vec<super::supabase::StoreItem> =
            self.client.get("items", &item_query).await?;

        if items.is_empty() {
            return Err(SupabaseError::Api {
                status: 404,
                body: "Item not found".to_string(),
            });
        }

        let item = &items[0];

        #[derive(Serialize)]
        struct UnequipUpdate {
            equipped: bool,
        }
        
        // Get all equipped items of same type to unequip
        let inventory: Vec<InventoryItemWithDetails> = self
            .client
            .get(
                "user_inventory",
                &format!(
                    "user_id=eq.{}&equipped=eq.true&select=item_id,owned,equipped,items(id,name,type)",
                    user_id
                ),
            )
            .await?;

        // Unequip items of the same type
        for inv_item in inventory {
            if let Some(details) = &inv_item.item {
                if details.item_type == item.item_type {
                    self.client
                        .update(
                            "user_inventory",
                            &format!("user_id=eq.{}&item_id=eq.{}", user_id, inv_item.item_id),
                            &UnequipUpdate { equipped: false },
                        )
                        .await?;
                }
            }
        }

        // Equip the requested item
        #[derive(Serialize)]
        struct EquipUpdate {
            equipped: bool,
        }

        self.client
            .update(
                "user_inventory",
                &format!("user_id=eq.{}&item_id=eq.{}", user_id, item_id),
                &EquipUpdate { equipped: true },
            )
            .await
    }

    /// Unequip an item
    pub async fn unequip_item(&self, user_id: Uuid, item_id: Uuid) -> Result<(), SupabaseError> {
        #[derive(Serialize)]
        struct UnequipUpdate {
            equipped: bool,
        }

        self.client
            .update(
                "user_inventory",
                &format!("user_id=eq.{}&item_id=eq.{}", user_id, item_id),
                &UnequipUpdate { equipped: false },
            )
            .await
    }

    /// Get all equipped items for a user
    pub async fn get_equipped_items(
        &self,
        user_id: Uuid,
    ) -> Result<Vec<UserInventoryItem>, SupabaseError> {
        let query = format!("user_id=eq.{}&equipped=eq.true", user_id);
        self.client.get("user_inventory", &query).await
    }
}
