subscription_id              = "97200cb2-456d-4471-876a-55f0a2bd8d54"
location                     = "northeurope"
backend_resource_group_name  = "fkukuck-fabro-tfstate-prod"
backend_storage_account_name = "fkukuckfabrotfprod01"
backend_container_name       = "tfstate"
backend_state_key            = "sandbox-prod.tfstate"
github_repository            = "fkukuck/fabro"
github_environment_name      = "production"
github_actions_identity_name = "fkukuck-fabro-gha-production"

tags = {
  environment = "bootstrap"
  managed_by  = "terraform"
}
