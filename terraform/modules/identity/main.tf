resource "azurerm_user_assigned_identity" "this" {
  name                = var.name
  location            = var.location
  resource_group_name = var.resource_group_name
  tags                = var.tags
}

resource "azurerm_role_assignment" "contributor" {
  count = var.contributor_scope == null ? 0 : 1

  scope                = var.contributor_scope
  role_definition_name = "Contributor"
  principal_id         = azurerm_user_assigned_identity.this.principal_id
}

resource "azurerm_role_assignment" "blob_data_contributor" {
  count = var.blob_data_scope == null ? 0 : 1

  scope                = var.blob_data_scope
  role_definition_name = "Storage Blob Data Contributor"
  principal_id         = azurerm_user_assigned_identity.this.principal_id
}

resource "azurerm_role_assignment" "acr_pull" {
  count = var.acr_pull_scope == null ? 0 : 1

  scope                = var.acr_pull_scope
  role_definition_name = "AcrPull"
  principal_id         = azurerm_user_assigned_identity.this.principal_id
}

resource "azurerm_role_assignment" "identity_attach" {
  count = var.identity_attach_scope == null ? 0 : 1

  scope                = var.identity_attach_scope
  role_definition_name = "Managed Identity Operator"
  principal_id         = coalesce(var.identity_attach_principal_id, azurerm_user_assigned_identity.this.principal_id)
}
