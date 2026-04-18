use std::fmt;

const CONTAINER_GROUP_PROVIDER: &str = "Microsoft.ContainerInstance";
const CONTAINER_GROUP_TYPE: &str = "containerGroups";
const ARM_API_VERSION: &str = "2023-05-01";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContainerGroupResourceId {
    pub subscription_id:      String,
    pub resource_group:       String,
    pub container_group_name: String,
}

impl ContainerGroupResourceId {
    #[must_use]
    pub fn new(
        subscription_id: String,
        resource_group: String,
        container_group_name: String,
    ) -> Self {
        Self {
            subscription_id,
            resource_group,
            container_group_name,
        }
    }

    pub fn parse(value: &str) -> Result<Self, String> {
        let parts: Vec<&str> = value.trim_matches('/').split('/').collect();
        if parts.len() != 8
            || parts[0] != "subscriptions"
            || parts[2] != "resourceGroups"
            || parts[4] != "providers"
            || parts[5] != CONTAINER_GROUP_PROVIDER
            || parts[6] != CONTAINER_GROUP_TYPE
        {
            return Err(format!(
                "invalid Azure container group resource ID: {value}"
            ));
        }

        Ok(Self::new(
            parts[1].to_string(),
            parts[3].to_string(),
            parts[7].to_string(),
        ))
    }

    #[must_use]
    pub fn arm_url(&self) -> String {
        format!("https://management.azure.com{self}?api-version={ARM_API_VERSION}")
    }

    #[must_use]
    pub fn arm_url_with_base(&self, arm_base_url: &str) -> String {
        format!(
            "{}{self}?api-version={ARM_API_VERSION}",
            arm_base_url.trim_end_matches('/')
        )
    }
}

impl fmt::Display for ContainerGroupResourceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "/subscriptions/{}/resourceGroups/{}/providers/{CONTAINER_GROUP_PROVIDER}/{CONTAINER_GROUP_TYPE}/{}",
            self.subscription_id, self.resource_group, self.container_group_name
        )
    }
}

#[cfg(test)]
mod tests {
    use super::ContainerGroupResourceId;

    #[test]
    fn parse_container_group_resource_id_round_trips() {
        let id = "/subscriptions/sub-1/resourceGroups/rg-1/providers/Microsoft.ContainerInstance/containerGroups/fabro-run-1";
        let parsed = ContainerGroupResourceId::parse(id).unwrap();
        assert_eq!(parsed.subscription_id, "sub-1");
        assert_eq!(parsed.resource_group, "rg-1");
        assert_eq!(parsed.container_group_name, "fabro-run-1");
        assert_eq!(parsed.to_string(), id);
    }
}
