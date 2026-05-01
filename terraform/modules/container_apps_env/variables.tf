variable "name" {
  type = string
}

variable "resource_group_name" {
  type = string
}

variable "location" {
  type = string
}

variable "infrastructure_subnet_id" {
  type = string
}

variable "storage_attachment_name" {
  type = string
}

variable "storage_account_name" {
  type = string
}

variable "storage_account_access_key" {
  type      = string
  sensitive = true
}

variable "server_storage_share_name" {
  type = string
}

variable "tags" {
  type    = map(string)
  default = {}
}
