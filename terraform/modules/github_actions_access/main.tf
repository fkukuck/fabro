resource "azurerm_role_assignment" "resource_group_contributor" {
  scope                = var.resource_group_id
  role_definition_name = "Contributor"
  principal_id         = var.principal_id
}

resource "azurerm_role_assignment" "acr_push" {
  scope                = var.acr_id
  role_definition_name = "AcrPush"
  principal_id         = var.principal_id
}

resource "azurerm_role_assignment" "identity_operator" {
  scope                = var.identity_id
  role_definition_name = "Managed Identity Operator"
  principal_id         = var.principal_id
}
