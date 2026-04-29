variable "enabled" {
  type    = bool
  default = true
}

variable "name" {
  type = string
}

variable "resource_group_name" {
  type = string
}

variable "container_app_environment_id" {
  type = string
}

variable "revision_mode" {
  type    = string
  default = "Single"
}

variable "image" {
  type = string
}

variable "registry_server" {
  type    = string
  default = null
}

variable "registry_username" {
  type      = string
  sensitive = true
  default   = null
}

variable "registry_password" {
  type      = string
  sensitive = true
  default   = null

  validation {
    condition = (
      (var.registry_server == null && var.registry_username == null && var.registry_password == null) ||
      (var.registry_server != null && var.registry_username != null && var.registry_password != null)
    )
    error_message = "Set registry_server, registry_username, and registry_password together, or leave all of them null."
  }
}

variable "cpu" {
  type    = number
  default = 2
}

variable "memory" {
  type    = string
  default = "4Gi"
}

variable "target_port" {
  type    = number
  default = 32276
}

variable "min_replicas" {
  type    = number
  default = 1

  validation {
    condition     = var.min_replicas == 1
    error_message = "Fabro Azure deployments currently require min_replicas = 1."
  }
}

variable "max_replicas" {
  type    = number
  default = 1

  validation {
    condition     = var.max_replicas == 1
    error_message = "Fabro Azure deployments currently require max_replicas = 1."
  }
}

variable "identity_id" {
  type = string
}

variable "identity_client_id" {
  type = string
}

variable "storage_volume_name" {
  type    = string
  default = "fabro-storage-volume"
}

variable "storage_attachment_name" {
  type = string
}

variable "storage_mount_path" {
  type    = string
  default = "/storage"
}

variable "tags" {
  type    = map(string)
  default = {}
}
