//! Provider-neutral attune resource models.

use std::collections::BTreeMap;

/// A provider-neutral DNS record set.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DnsRecord {
    pub zone: String,
    pub record_type: String,
    pub name: String,
    pub ttl: i64,
    pub values: Vec<String>,
}

impl DnsRecord {
    pub fn key(&self) -> String {
        format!(
            "{}|{}|{}",
            self.zone,
            self.record_type.to_uppercase(),
            self.name
        )
    }

    pub fn same_data(&self, other: &Self) -> bool {
        let normalize = |values: &[String]| {
            let mut values: Vec<_> = values.iter().map(|v| v.trim().to_owned()).collect();
            values.sort_unstable();
            values
        };
        self.ttl == other.ttl && normalize(&self.values) == normalize(&other.values)
    }
}

/// A security group.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SecurityGroup {
    pub name: String,
    pub owners: Vec<String>,
    pub members: Vec<String>,
}

/// An application registration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppRegistration {
    pub name: String,
    pub owners: Vec<String>,
    pub service_principal: bool,
}

/// An Azure RBAC scope.
#[derive(Clone, Debug, Default, Eq, PartialEq, Ord, PartialOrd)]
pub struct Scope {
    pub management_group: String,
    pub subscription: String,
    pub resource_group: String,
    pub dns_zone: String,
}

impl Scope {
    pub fn arm_id(&self, default_subscription: &str) -> Result<String, String> {
        if !self.management_group.is_empty() {
            return Ok(format!(
                "/providers/Microsoft.Management/managementGroups/{}",
                self.management_group
            ));
        }
        let subscription = if self.subscription.is_empty() {
            default_subscription
        } else {
            &self.subscription
        };
        if subscription.is_empty() {
            return Err("scope needs a subscription; set it in the spec or target config".into());
        }
        if !self.dns_zone.is_empty() && self.resource_group.is_empty() {
            return Err("dnsZone scope requires a resourceGroup".into());
        }
        let mut id = format!("/subscriptions/{subscription}");
        if !self.resource_group.is_empty() {
            id.push_str(&format!("/resourceGroups/{}", self.resource_group));
        }
        if !self.dns_zone.is_empty() {
            id.push_str(&format!(
                "/providers/Microsoft.Network/dnsZones/{}",
                self.dns_zone
            ));
        }
        Ok(id)
    }
}

/// A custom role definition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RoleDefinition {
    pub name: String,
    pub description: String,
    pub assignable_scopes: Vec<Scope>,
    pub actions: Vec<String>,
    pub not_actions: Vec<String>,
    pub data_actions: Vec<String>,
    pub not_data_actions: Vec<String>,
}

/// A role assignment.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RoleAssignment {
    pub principal: String,
    pub principal_type: String,
    pub role: String,
    pub scope: Scope,
}

/// A resource group whose declared tags use merge semantics.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResourceGroup {
    pub name: String,
    pub location: String,
    pub tags: BTreeMap<String, String>,
}

/// All supported desired resources.
#[derive(Clone, Debug, Default)]
pub struct Bundle {
    pub dns: Vec<DnsRecord>,
    pub groups: Vec<SecurityGroup>,
    pub apps: Vec<AppRegistration>,
    pub role_definitions: Vec<RoleDefinition>,
    pub role_assignments: Vec<RoleAssignment>,
    pub resource_groups: Vec<ResourceGroup>,
}

impl Bundle {
    pub fn len(&self) -> usize {
        self.dns.len()
            + self.groups.len()
            + self.apps.len()
            + self.role_definitions.len()
            + self.role_assignments.len()
            + self.resource_groups.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

pub(crate) fn normalized(values: &[String]) -> Vec<&str> {
    let mut values: Vec<_> = values.iter().map(|value| value.as_str()).collect();
    values.sort_unstable();
    values.dedup();
    values
}

pub(crate) fn tags_satisfy(
    declared: &BTreeMap<String, String>,
    live: &BTreeMap<String, String>,
) -> bool {
    declared
        .iter()
        .all(|(key, value)| live.get(key) == Some(value))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_requires_resource_group_for_dns_zone() {
        let scope = Scope {
            dns_zone: "example.com".into(),
            ..Scope::default()
        };
        assert!(
            scope
                .arm_id("00000000-0000-0000-0000-000000000001")
                .is_err()
        );
    }

    #[test]
    fn dns_values_are_order_insensitive() {
        let left = DnsRecord {
            zone: "example.com".into(),
            record_type: "TXT".into(),
            name: "@".into(),
            ttl: 60,
            values: vec!["b".into(), "a".into()],
        };
        let mut right = left.clone();
        right.values.reverse();
        assert!(left.same_data(&right));
    }
}
