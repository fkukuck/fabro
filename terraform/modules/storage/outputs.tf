output "id" {
  value = azurerm_storage_account.this.id
}

output "account_name" {
  value = azurerm_storage_account.this.name
}

output "primary_access_key" {
  value     = azurerm_storage_account.this.primary_access_key
  sensitive = true
}

output "workspace_share_name" {
  value = azurerm_storage_share.workspace.name
}

output "server_storage_share_name" {
  value = azurerm_storage_share.server.name
}
