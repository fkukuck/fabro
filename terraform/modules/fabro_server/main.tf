locals {
  optional_secret_env = {
    GITHUB_TOKEN      = var.github_token
    OPENAI_API_KEY    = var.openai_api_key
    ANTHROPIC_API_KEY = var.anthropic_api_key
  }

  optional_secrets = {
    for name, value in local.optional_secret_env : name => value
    if value != null && value != ""
  }

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

  secret {
    name  = "fabro-dev-token"
    value = var.fabro_dev_token
  }

  secret {
    name  = "session-secret"
    value = var.session_secret
  }

  secret {
    name  = "azure-storage-key"
    value = var.azure_storage_key
  }

  secret {
    name  = "azure-acr-username"
    value = var.azure_acr_username
  }

  secret {
    name  = "azure-acr-password"
    value = var.azure_acr_password
  }

  dynamic "secret" {
    for_each = local.image_registry_enabled ? [1] : []
    content {
      name  = "image-registry-password"
      value = var.registry_password
    }
  }

  dynamic "secret" {
    for_each = local.optional_secrets
    content {
      name  = lower(replace(secret.key, "_", "-"))
      value = secret.value
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
        name        = "FABRO_DEV_TOKEN"
        secret_name = "fabro-dev-token"
      }

      env {
        name        = "SESSION_SECRET"
        secret_name = "session-secret"
      }

      env {
        name  = "FABRO_AZURE_SUBSCRIPTION_ID"
        value = var.azure_subscription_id
      }

      env {
        name  = "FABRO_AZURE_RESOURCE_GROUP"
        value = var.azure_resource_group
      }

      env {
        name  = "FABRO_AZURE_LOCATION"
        value = var.azure_location
      }

      env {
        name  = "FABRO_AZURE_SANDBOX_SUBNET_ID"
        value = var.azure_sandbox_subnet_id
      }

      env {
        name  = "FABRO_AZURE_STORAGE_ACCOUNT"
        value = var.azure_storage_account
      }

      env {
        name  = "FABRO_AZURE_STORAGE_SHARE"
        value = var.azure_storage_share
      }

      env {
        name        = "FABRO_AZURE_STORAGE_KEY"
        secret_name = "azure-storage-key"
      }

      env {
        name  = "FABRO_AZURE_ACR_SERVER"
        value = var.azure_acr_server
      }

      env {
        name        = "FABRO_AZURE_ACR_USERNAME"
        secret_name = "azure-acr-username"
      }

      env {
        name        = "FABRO_AZURE_ACR_PASSWORD"
        secret_name = "azure-acr-password"
      }

      env {
        name  = "FABRO_AZURE_SANDBOXD_PORT"
        value = tostring(var.azure_sandboxd_port)
      }

      env {
        name  = "AZURE_CLIENT_ID"
        value = var.identity_client_id
      }

      dynamic "env" {
        for_each = local.optional_secrets
        content {
          name        = env.key
          secret_name = lower(replace(env.key, "_", "-"))
        }
      }

      volume_mounts {
        name = var.storage_volume_name
        path = var.storage_mount_path
      }
    }
  }
}
