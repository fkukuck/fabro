output "vnet_id" {
  value = azurerm_virtual_network.this.id
}

output "aca_subnet_id" {
  value = azurerm_subnet.aca.id
}

output "aci_subnet_id" {
  value = azurerm_subnet.aci.id
}
