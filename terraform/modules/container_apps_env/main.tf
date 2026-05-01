resource "azurerm_container_app_environment" "this" {
  name                     = var.name
  location                 = var.location
  resource_group_name      = var.resource_group_name
  infrastructure_subnet_id = var.infrastructure_subnet_id
  tags                     = var.tags

  lifecycle {
    ignore_changes = [
      infrastructure_resource_group_name,
      workload_profile,
    ]
  }
}

resource "azurerm_container_app_environment_storage" "server" {
  name                         = var.storage_attachment_name
  container_app_environment_id = azurerm_container_app_environment.this.id
  account_name                 = var.storage_account_name
  access_key                   = var.storage_account_access_key
  share_name                   = var.server_storage_share_name
  access_mode                  = "ReadWrite"
}
