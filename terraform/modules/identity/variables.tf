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
  type    = string
  default = null
}

variable "blob_data_scope" {
  type    = string
  default = null
}

variable "acr_pull_scope" {
  type    = string
  default = null
}

variable "identity_attach_scope" {
  type    = string
  default = null
}

variable "identity_attach_principal_id" {
  type    = string
  default = null
}

variable "tags" {
  type    = map(string)
  default = {}
}
