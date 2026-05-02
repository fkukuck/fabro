output "id" {
  value = var.enabled ? azurerm_container_app.this[0].id : null
}

output "name" {
  value = var.enabled ? azurerm_container_app.this[0].name : null
}

output "latest_revision_name" {
  value = var.enabled ? azurerm_container_app.this[0].latest_revision_name : null
}

output "fqdn" {
  value = var.enabled ? azurerm_container_app.this[0].ingress[0].fqdn : null
}

output "url" {
  value = var.enabled ? format("https://%s", azurerm_container_app.this[0].ingress[0].fqdn) : null
}
