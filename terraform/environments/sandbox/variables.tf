variable "subscription_id" {
  description = "Azure subscription ID used for all resources in this environment."
  type        = string
}

variable "location" {
  type = string
}

variable "resource_group_name" {
  type = string
}

variable "tags" {
  type    = map(string)
  default = {}
}

variable "vnet_name" {
  type = string
}

variable "vnet_cidr" {
  type = string
}

variable "aca_subnet_name" {
  type = string
}

variable "aca_subnet_cidr" {
  type = string
}

variable "aci_subnet_name" {
  type = string
}

variable "aci_subnet_cidr" {
  type = string
}

variable "storage_account_name" {
  type = string
}

variable "server_storage_share_name" {
  type = string
}

variable "blob_data_container_name" {
  type    = string
  default = "fabro-data"
}

variable "acr_name" {
  type = string
}

variable "identity_name" {
  type = string
}

variable "container_apps_environment_name" {
  type = string
}

variable "container_apps_environment_storage_name" {
  type    = string
  default = "fabrostorage"
}

variable "fabro_server_name" {
  type = string
}

variable "fabro_server_enabled" {
  type    = bool
  default = true
}

variable "fabro_server_image" {
  type        = string
  description = "Immutable image reference for fabro-server, published in the environment ACR."
}

variable "fabro_server_cpu" {
  type    = number
  default = 2
}

variable "fabro_server_memory" {
  type    = string
  default = "4Gi"
}

variable "github_actions_principal_id" {
  type        = string
  description = "Principal ID of the bootstrap-created GitHub Actions managed identity."
  default     = null
}
