resource "azurerm_resource_group" "backend" {
  name     = var.backend_resource_group_name
  location = var.location
  tags     = var.tags
}

resource "azurerm_storage_account" "backend" {
  name                     = var.backend_storage_account_name
  resource_group_name      = azurerm_resource_group.backend.name
  location                 = azurerm_resource_group.backend.location
  account_tier             = "Standard"
  account_replication_type = "LRS"
  tags                     = var.tags
}

resource "azurerm_storage_container" "backend" {
  name                  = var.backend_container_name
  storage_account_id    = azurerm_storage_account.backend.id
  container_access_type = "private"
}

data "azurerm_client_config" "current" {}

resource "azurerm_user_assigned_identity" "github_actions" {
  name                = var.github_actions_identity_name
  resource_group_name = azurerm_resource_group.backend.name
  location            = azurerm_resource_group.backend.location
  tags                = var.tags
}

resource "azurerm_federated_identity_credential" "github_actions" {
  name                = "github-actions-${var.github_environment_name}"
  resource_group_name = azurerm_resource_group.backend.name
  parent_id           = azurerm_user_assigned_identity.github_actions.id
  audience            = ["api://AzureADTokenExchange"]
  issuer              = "https://token.actions.githubusercontent.com"
  subject             = "repo:${var.github_repository}:environment:${var.github_environment_name}"
}

resource "azurerm_role_assignment" "backend_blob_access" {
  scope                = azurerm_storage_account.backend.id
  role_definition_name = "Storage Blob Data Contributor"
  principal_id         = azurerm_user_assigned_identity.github_actions.principal_id
}

resource "azurerm_role_assignment" "backend_reader" {
  scope                = azurerm_storage_account.backend.id
  role_definition_name = "Reader"
  principal_id         = azurerm_user_assigned_identity.github_actions.principal_id
}

resource "azurerm_role_assignment" "backend_key_operator" {
  scope                = azurerm_storage_account.backend.id
  role_definition_name = "Storage Account Key Operator Service Role"
  principal_id         = azurerm_user_assigned_identity.github_actions.principal_id
}
