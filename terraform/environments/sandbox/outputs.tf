output "resource_group_name" {
  value = module.resource_group.name
}

output "container_apps_environment_name" {
  value = module.container_apps_env.name
}

output "aci_subnet_id" {
  value = module.network.aci_subnet_id
}

output "server_storage_share_name" {
  value = module.storage.server_storage_share_name
}

output "acr_login_server" {
  value = module.acr.login_server
}

output "acr_name" {
  value = module.acr.name
}

output "managed_identity_client_id" {
  value = module.identity.client_id
}

output "fabro_server_fqdn" {
  value = module.fabro_server.fqdn
}

output "fabro_server_url" {
  value = module.fabro_server.url
}
