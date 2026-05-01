variable "subscription_id" {
  type = string
}

variable "location" {
  type = string
}

variable "backend_resource_group_name" {
  type = string
}

variable "backend_storage_account_name" {
  type = string
}

variable "backend_container_name" {
  type = string
}

variable "backend_state_key" {
  type = string
}

variable "github_repository" {
  type = string
}

variable "github_environment_name" {
  type = string
}

variable "github_actions_identity_name" {
  type = string
}

variable "tags" {
  type    = map(string)
  default = {}
}
