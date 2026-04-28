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
  workspace_share_name      = var.workspace_share_name
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
  registry_server              = var.fabro_server_image_registry_server
  registry_username            = var.fabro_server_image_registry_username
  registry_password            = var.fabro_server_image_registry_password
  cpu                          = var.fabro_server_cpu
  memory                       = var.fabro_server_memory
  identity_id                  = module.identity.id
  identity_client_id           = module.identity.client_id
  storage_attachment_name      = module.container_apps_env.storage_attachment_name
  azure_subscription_id        = var.subscription_id
  azure_resource_group         = module.resource_group.name
  azure_location               = module.resource_group.location
  azure_sandbox_subnet_id      = module.network.aci_subnet_id
  azure_storage_account        = module.storage.account_name
  azure_storage_share          = module.storage.workspace_share_name
  azure_storage_key            = module.storage.primary_access_key
  azure_acr_server             = module.acr.login_server
  azure_acr_username           = module.acr.admin_username
  azure_acr_password           = module.acr.admin_passwords
  azure_sandboxd_port          = var.azure_sandboxd_port
  fabro_dev_token              = var.fabro_dev_token
  session_secret               = var.session_secret
  github_token                 = var.github_token
  openai_api_key               = var.openai_api_key
  anthropic_api_key            = var.anthropic_api_key
  tags                         = var.tags
}
