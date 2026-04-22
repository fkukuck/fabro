resource "azurerm_storage_account" "this" {
  name                     = var.account_name
  resource_group_name      = var.resource_group_name
  location                 = var.location
  account_tier             = var.account_tier
  account_replication_type = var.account_replication_type
  account_kind             = "StorageV2"
  large_file_share_enabled = var.enable_large_file_share
  tags                     = var.tags
}

resource "azurerm_storage_share" "workspace" {
  name               = var.workspace_share_name
  quota              = var.workspace_share_quota_gb
  storage_account_id = azurerm_storage_account.this.id
}

resource "azurerm_storage_share" "server" {
  name               = var.server_storage_share_name
  quota              = var.server_storage_share_quota_gb
  storage_account_id = azurerm_storage_account.this.id
}
