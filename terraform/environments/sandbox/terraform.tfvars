subscription_id                         = "97200cb2-456d-4471-876a-55f0a2bd8d54"
location                                = "northeurope"
resource_group_name                     = "fkukuck-fabro-prod"
vnet_name                               = "fkukuck-fabro-vnet"
vnet_cidr                               = "10.10.0.0/16"
aca_subnet_name                         = "aca-subnet"
aca_subnet_cidr                         = "10.10.0.0/23"
aci_subnet_name                         = "aci-subnet"
aci_subnet_cidr                         = "10.10.2.0/24"
storage_account_name                    = "fkukuckfabroprod01"
server_storage_share_name               = "fabro-storage"
acr_name                                = "fkukuckfabroprod01"
identity_name                           = "fkukuck-fabro-server"
container_apps_environment_name         = "fkukuck-fabro-env"
container_apps_environment_storage_name = "fabrostorage"
fabro_server_name                       = "fkukuck-fabro-server"
fabro_server_enabled                    = false
fabro_server_image                      = "example.azurecr.io/fabro-server:bootstrap"
fabro_server_cpu                        = 2
fabro_server_memory                     = "4Gi"
github_actions_principal_id             = "be3c8c5c-9478-441e-9a6f-e58ddb8746ba"

tags = {
  environment = "sandbox"
  managed_by  = "terraform"
}
