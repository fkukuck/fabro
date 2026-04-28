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

variable "workspace_share_name" {
  type = string
}

variable "server_storage_share_name" {
  type = string
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
  description = "Immutable image reference for fabro-server, from ACR or GHCR."
}

variable "fabro_server_image_registry_server" {
  type        = string
  default     = null
  description = "Optional registry host for private fabro-server images, for example an ACR login server or ghcr.io."
}

variable "fabro_server_image_registry_username" {
  type      = string
  sensitive = true
  default   = null
}

variable "fabro_server_image_registry_password" {
  type      = string
  sensitive = true
  default   = null

  validation {
    condition = (
      (var.fabro_server_image_registry_server == null && var.fabro_server_image_registry_username == null && var.fabro_server_image_registry_password == null) ||
      (var.fabro_server_image_registry_server != null && var.fabro_server_image_registry_username != null && var.fabro_server_image_registry_password != null)
    )
    error_message = "Set fabro_server_image_registry_server, fabro_server_image_registry_username, and fabro_server_image_registry_password together, or leave all of them null."
  }
}

variable "fabro_server_cpu" {
  type    = number
  default = 1
}

variable "fabro_server_memory" {
  type    = string
  default = "2Gi"
}

variable "fabro_dev_token" {
  type      = string
  sensitive = true
}

variable "session_secret" {
  type      = string
  sensitive = true
}

variable "github_token" {
  type      = string
  sensitive = true
  default   = null
}

variable "openai_api_key" {
  type      = string
  sensitive = true
  default   = null
}

variable "anthropic_api_key" {
  type      = string
  sensitive = true
  default   = null
}

variable "azure_sandboxd_port" {
  type    = number
  default = 7777
}
