module "resource_group" {
  source   = "../../modules/resource_group"
  name     = var.resource_group_name
  location = var.location
  tags     = var.tags
}

module "network" {
  source              = "../../modules/network"
  resource_group_name = module.resource_group.name
  location            = module.resource_group.location
  vnet_name           = var.vnet_name
  vnet_cidr           = var.vnet_cidr
  aca_subnet_name     = var.aca_subnet_name
  aca_subnet_cidr     = var.aca_subnet_cidr
  aci_subnet_name     = var.aci_subnet_name
  aci_subnet_cidr     = var.aci_subnet_cidr
  tags                = var.tags
}

module "storage" {
  source                    = "../../modules/storage"
  resource_group_name       = module.resource_group.name
  location                  = module.resource_group.location
  account_name              = var.storage_account_name
  server_storage_share_name = var.server_storage_share_name
  tags                      = var.tags
}

module "acr" {
  source              = "../../modules/acr"
  resource_group_name = module.resource_group.name
  location            = module.resource_group.location
  name                = var.acr_name
  tags                = var.tags
}

module "github_actions_access" {
  count = var.github_actions_principal_id == null ? 0 : 1

  source            = "../../modules/github_actions_access"
  principal_id      = var.github_actions_principal_id
  resource_group_id = module.resource_group.id
  acr_id            = module.acr.id
  identity_id       = module.identity.id
}

module "identity" {
  source              = "../../modules/identity"
  resource_group_name = module.resource_group.name
  location            = module.resource_group.location
  name                = var.identity_name
  contributor_scope   = module.resource_group.id
  tags                = var.tags
}

module "container_apps_env" {
  source                     = "../../modules/container_apps_env"
  name                       = var.container_apps_environment_name
  resource_group_name        = module.resource_group.name
  location                   = module.resource_group.location
  infrastructure_subnet_id   = module.network.aca_subnet_id
  storage_attachment_name    = var.container_apps_environment_storage_name
  storage_account_name       = module.storage.account_name
  storage_account_access_key = module.storage.primary_access_key
  server_storage_share_name  = module.storage.server_storage_share_name
  tags                       = var.tags
}

module "fabro_server" {
  source                       = "../../modules/fabro_server"
  enabled                      = var.fabro_server_enabled
  name                         = var.fabro_server_name
  resource_group_name          = module.resource_group.name
  container_app_environment_id = module.container_apps_env.id
  image                        = var.fabro_server_image
  registry_server              = module.acr.login_server
  registry_username            = module.acr.admin_username
  registry_password            = module.acr.admin_passwords
  cpu                          = var.fabro_server_cpu
  memory                       = var.fabro_server_memory
  identity_id                  = module.identity.id
  identity_client_id           = module.identity.client_id
  storage_attachment_name      = module.container_apps_env.storage_attachment_name
  tags                         = var.tags
}
