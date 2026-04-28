# Sandbox Environment

This environment provisions the documented Fabro Azure topology using local Terraform state by default.

## Bootstrap flow

1. Copy `terraform.tfvars.example` to `terraform.tfvars` and fill in real values.
2. Set `fabro_server_enabled = false` if the first app image does not exist yet.
3. Run `terraform init`.
4. Run `terraform apply`.
5. Build and push a `fabro-server` image to ACR or choose an immutable GHCR tag.
6. Set `fabro_server_enabled = true` and update `fabro_server_image`.
7. Run `terraform apply` again.

## Steady-state deploy flow

1. Build and push a new immutable image tag.
2. Update `fabro_server_image`.
3. Run `terraform apply`.

## Private image registries

If the `fabro-server` image lives in a private registry, set all three of these variables together:

- `fabro_server_image_registry_server`
- `fabro_server_image_registry_username`
- `fabro_server_image_registry_password`

Leave them unset for public GHCR images.

## Local state

This environment uses local Terraform state unless you configure a backend. That is fine for one operator getting started.

For team or CI usage, move state to a shared remote backend later.

## Notes

- Storage account and ACR names must be globally unique and meet Azure naming rules.
- Keep the Container App at one replica.
- ACR is provisioned even when `fabro-server` runs from GHCR because Azure sandbox images still use ACR.
- Azure ACI sandboxes now use ephemeral `emptyDir` storage for `/workspace`; only the server's `/storage` share is persistent.
- v1 is create-only. Attaching Terraform to pre-existing Azure resources is a future enhancement.
