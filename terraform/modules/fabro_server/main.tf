locals {
  image_registry_enabled = (
    var.registry_server != null &&
    var.registry_username != null &&
    var.registry_password != null
  )
}

resource "azurerm_container_app" "this" {
  count = var.enabled ? 1 : 0

  name                         = var.name
  resource_group_name          = var.resource_group_name
  container_app_environment_id = var.container_app_environment_id
  revision_mode                = var.revision_mode
  tags                         = var.tags

  identity {
    type         = "UserAssigned"
    identity_ids = [var.identity_id]
  }

  dynamic "secret" {
    for_each = local.image_registry_enabled ? [1] : []
    content {
      name  = "image-registry-password"
      value = var.registry_password
    }
  }

  dynamic "registry" {
    for_each = local.image_registry_enabled ? [1] : []
    content {
      server               = var.registry_server
      username             = var.registry_username
      password_secret_name = "image-registry-password"
    }
  }

  ingress {
    allow_insecure_connections = false
    external_enabled           = true
    target_port                = var.target_port

    traffic_weight {
      latest_revision = true
      percentage      = 100
    }
  }

  template {
    min_replicas = var.min_replicas
    max_replicas = var.max_replicas

    volume {
      name         = var.storage_volume_name
      storage_name = var.storage_attachment_name
      storage_type = "AzureFile"
    }

    container {
      name   = "fabro-server"
      image  = var.image
      cpu    = var.cpu
      memory = var.memory

      env {
        name  = "AZURE_CLIENT_ID"
        value = var.identity_client_id
      }

      env {
        name  = "FABRO_SKIP_PRIV_DROP"
        value = "1"
      }

      env {
        name  = "FABRO_SERVER_RUNTIME_DIR"
        value = "/tmp/fabro-runtime"
      }

      volume_mounts {
        name = var.storage_volume_name
        path = var.storage_mount_path
      }
    }
  }
}
