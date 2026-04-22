variable "resource_group_name" {
  type = string
}

variable "location" {
  type = string
}

variable "account_name" {
  type = string
}

variable "account_tier" {
  type    = string
  default = "Standard"
}

variable "account_replication_type" {
  type    = string
  default = "LRS"
}

variable "enable_large_file_share" {
  type    = bool
  default = true
}

variable "workspace_share_name" {
  type = string
}

variable "workspace_share_quota_gb" {
  type    = number
  default = 100
}

variable "server_storage_share_name" {
  type = string
}

variable "server_storage_share_quota_gb" {
  type    = number
  default = 100
}

variable "tags" {
  type    = map(string)
  default = {}
}
