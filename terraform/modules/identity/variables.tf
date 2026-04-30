variable "resource_group_name" {
  type = string
}

variable "location" {
  type = string
}

variable "name" {
  type = string
}

variable "contributor_scope" {
  type = string
}

variable "blob_data_scope" {
  type = string
}

variable "acr_pull_scope" {
  type = string
}

variable "tags" {
  type    = map(string)
  default = {}
}
