output "github_actions_client_id" {
  value = azurerm_user_assigned_identity.github_actions.client_id
}

output "github_actions_principal_id" {
  value = azurerm_user_assigned_identity.github_actions.principal_id
}

output "tenant_id" {
  value = data.azurerm_client_config.current.tenant_id
}

output "subscription_id" {
  value = data.azurerm_client_config.current.subscription_id
}

output "backend_resource_group_name" {
  value = azurerm_resource_group.backend.name
}

output "backend_storage_account_name" {
  value = azurerm_storage_account.backend.name
}

output "backend_container_name" {
  value = azurerm_storage_container.backend.name
}

output "backend_state_key" {
  value = var.backend_state_key
}
