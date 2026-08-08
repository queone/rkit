//! Stateless desired/live reconciliation.

use super::model::*;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Action {
    Create,
    Update,
    Delete,
}

impl Action {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Create => "create",
            Self::Update => "update",
            Self::Delete => "delete",
        }
    }
}

#[derive(Clone, Debug)]
pub enum Target {
    Dns(DnsRecord),
    Group(SecurityGroup),
    App(AppRegistration),
    RoleDefinition(RoleDefinition),
    RoleAssignment {
        desired: RoleAssignment,
        principal_id: String,
        role_id: String,
        scope_id: String,
    },
    ResourceGroup(ResourceGroup),
}

#[derive(Clone, Debug)]
pub struct Change {
    pub kind: &'static str,
    pub action: Action,
    pub key: String,
    pub summary: String,
    pub target: Target,
}

#[derive(Clone, Debug, Default)]
pub struct Options {
    pub subscription: String,
    pub kind: String,
    pub prune_dns: bool,
    pub prune_identities: bool,
    pub prune_roles: bool,
    pub prune_resource_groups: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LiveAssignment {
    pub id: String,
    pub principal_id: String,
    pub role_id: String,
    pub scope: String,
}

pub trait Provider {
    fn ensure_zone(&mut self, zone: &str) -> Result<(), String>;
    fn list_dns(&mut self, zone: &str) -> Result<Vec<DnsRecord>, String>;
    fn put_dns(&mut self, record: &DnsRecord) -> Result<(), String>;
    fn delete_dns(&mut self, record: &DnsRecord) -> Result<(), String>;
    fn get_group(&mut self, name: &str) -> Result<Option<SecurityGroup>, String>;
    fn list_group_names(&mut self) -> Result<Vec<String>, String>;
    fn put_group(&mut self, group: &SecurityGroup) -> Result<(), String>;
    fn delete_group(&mut self, name: &str) -> Result<(), String>;
    fn get_app(&mut self, name: &str) -> Result<Option<AppRegistration>, String>;
    fn list_app_names(&mut self) -> Result<Vec<String>, String>;
    fn put_app(&mut self, app: &AppRegistration) -> Result<(), String>;
    fn delete_app(&mut self, name: &str) -> Result<(), String>;
    fn list_role_definitions(&mut self, subscription: &str) -> Result<Vec<RoleDefinition>, String>;
    fn put_role_definition(
        &mut self,
        role: &RoleDefinition,
        subscription: &str,
    ) -> Result<(), String>;
    fn delete_role_definition(&mut self, name: &str, subscription: &str) -> Result<(), String>;
    fn resolve_principal(&mut self, name: &str, principal_type: &str) -> Result<String, String>;
    fn resolve_role(&mut self, name: &str, subscription: &str) -> Result<String, String>;
    fn list_role_assignments(&mut self, scope: &str) -> Result<Vec<LiveAssignment>, String>;
    fn put_role_assignment(
        &mut self,
        principal: &str,
        role: &str,
        scope: &str,
    ) -> Result<(), String>;
    fn delete_role_assignment(&mut self, assignment: &LiveAssignment) -> Result<(), String>;
    fn list_resource_groups(&mut self) -> Result<Vec<ResourceGroup>, String>;
    fn put_resource_group(&mut self, group: &ResourceGroup) -> Result<(), String>;
    fn delete_resource_group(&mut self, name: &str) -> Result<(), String>;
}

fn wanted(options: &Options, kind: &str) -> bool {
    options.kind.is_empty() || options.kind == kind
}
fn sorted(values: &[String]) -> Vec<&str> {
    normalized(values)
}

pub fn plan<P: Provider>(
    provider: &mut P,
    bundle: &Bundle,
    options: &Options,
) -> Result<Vec<Change>, String> {
    let mut changes = Vec::new();
    if wanted(options, "resourceGroup") && !bundle.resource_groups.is_empty() {
        let live = provider.list_resource_groups()?;
        let desired: BTreeSet<_> = bundle
            .resource_groups
            .iter()
            .map(|item| item.name.as_str())
            .collect();
        for item in &bundle.resource_groups {
            match live.iter().find(|candidate| candidate.name == item.name) {
                None => changes.push(change(
                    "resourceGroup",
                    Action::Create,
                    format!("resourceGroup|{}", item.name),
                    "missing",
                    Target::ResourceGroup(item.clone()),
                )),
                Some(current)
                    if current.location != item.location
                        || !tags_satisfy(&item.tags, &current.tags) =>
                {
                    let mut merged = current.tags.clone();
                    merged.extend(item.tags.clone());
                    let mut item = item.clone();
                    item.tags = merged;
                    changes.push(change(
                        "resourceGroup",
                        Action::Update,
                        format!("resourceGroup|{}", item.name),
                        "location or declared tags differ",
                        Target::ResourceGroup(item),
                    ));
                }
                Some(_) => {}
            }
        }
        if options.prune_resource_groups {
            for item in live {
                if !desired.contains(item.name.as_str()) {
                    changes.push(change(
                        "resourceGroup",
                        Action::Delete,
                        format!("resourceGroup|{}", item.name),
                        "absent from specs",
                        Target::ResourceGroup(item),
                    ));
                }
            }
        }
    }
    if wanted(options, "securityGroup") && !bundle.groups.is_empty() {
        for item in &bundle.groups {
            match provider.get_group(&item.name)? {
                None => changes.push(change(
                    "securityGroup",
                    Action::Create,
                    format!("securityGroup|{}", item.name),
                    "missing",
                    Target::Group(item.clone()),
                )),
                Some(current)
                    if sorted(&current.owners) != sorted(&item.owners)
                        || sorted(&current.members) != sorted(&item.members) =>
                {
                    changes.push(change(
                        "securityGroup",
                        Action::Update,
                        format!("securityGroup|{}", item.name),
                        "owners or members differ",
                        Target::Group(item.clone()),
                    ))
                }
                Some(_) => {}
            }
        }
        if options.prune_identities {
            let desired: BTreeSet<_> = bundle
                .groups
                .iter()
                .map(|item| item.name.as_str())
                .collect();
            for name in provider.list_group_names()? {
                if !desired.contains(name.as_str()) {
                    changes.push(change(
                        "securityGroup",
                        Action::Delete,
                        format!("securityGroup|{name}"),
                        "absent from specs",
                        Target::Group(SecurityGroup {
                            name,
                            owners: vec![],
                            members: vec![],
                        }),
                    ));
                }
            }
        }
    }
    if wanted(options, "appRegistration") && !bundle.apps.is_empty() {
        for item in &bundle.apps {
            match provider.get_app(&item.name)? {
                None => changes.push(change(
                    "appRegistration",
                    Action::Create,
                    format!("appRegistration|{}", item.name),
                    "missing",
                    Target::App(item.clone()),
                )),
                Some(current)
                    if sorted(&current.owners) != sorted(&item.owners)
                        || current.service_principal != item.service_principal =>
                {
                    changes.push(change(
                        "appRegistration",
                        Action::Update,
                        format!("appRegistration|{}", item.name),
                        "owners or service principal differ",
                        Target::App(item.clone()),
                    ))
                }
                Some(_) => {}
            }
        }
        if options.prune_identities {
            let desired: BTreeSet<_> = bundle.apps.iter().map(|item| item.name.as_str()).collect();
            for name in provider.list_app_names()? {
                if !desired.contains(name.as_str()) {
                    changes.push(change(
                        "appRegistration",
                        Action::Delete,
                        format!("appRegistration|{name}"),
                        "absent from specs",
                        Target::App(AppRegistration {
                            name,
                            owners: vec![],
                            service_principal: false,
                        }),
                    ));
                }
            }
        }
    }
    if wanted(options, "roleDefinition") && !bundle.role_definitions.is_empty() {
        let live = provider.list_role_definitions(&options.subscription)?;
        let desired: BTreeSet<_> = bundle
            .role_definitions
            .iter()
            .map(|item| item.name.as_str())
            .collect();
        for item in &bundle.role_definitions {
            match live.iter().find(|candidate| candidate.name == item.name) {
                None => changes.push(change(
                    "roleDefinition",
                    Action::Create,
                    format!("roleDefinition|{}", item.name),
                    "missing",
                    Target::RoleDefinition(item.clone()),
                )),
                Some(current) if !same_role_definition(current, item, &options.subscription) => {
                    changes.push(change(
                        "roleDefinition",
                        Action::Update,
                        format!("roleDefinition|{}", item.name),
                        "permissions or scopes differ",
                        Target::RoleDefinition(item.clone()),
                    ))
                }
                Some(_) => {}
            }
        }
        if options.prune_roles {
            for item in live {
                if !desired.contains(item.name.as_str()) {
                    changes.push(change(
                        "roleDefinition",
                        Action::Delete,
                        format!("roleDefinition|{}", item.name),
                        "absent from specs",
                        Target::RoleDefinition(item),
                    ));
                }
            }
        }
    }
    if wanted(options, "roleAssignment") && !bundle.role_assignments.is_empty() {
        let mut desired_by_scope: BTreeMap<String, Vec<(RoleAssignment, String, String)>> =
            BTreeMap::new();
        for item in &bundle.role_assignments {
            let principal_id = provider.resolve_principal(&item.principal, &item.principal_type)?;
            let role_id = provider.resolve_role(&item.role, &options.subscription)?;
            let scope_id = item.scope.arm_id(&options.subscription)?;
            desired_by_scope.entry(scope_id).or_default().push((
                item.clone(),
                principal_id,
                role_id,
            ));
        }
        for (scope_id, desired) in desired_by_scope {
            let live = provider.list_role_assignments(&scope_id)?;
            for (item, principal_id, role_id) in &desired {
                let found = live.iter().any(|candidate| {
                    eq_id(&candidate.principal_id, principal_id)
                        && eq_id(&candidate.role_id, role_id)
                });
                if !found {
                    changes.push(change(
                        "roleAssignment",
                        Action::Create,
                        format!(
                            "roleAssignment|{}|{}|{}",
                            item.principal, item.role, scope_id
                        ),
                        "missing",
                        Target::RoleAssignment {
                            desired: item.clone(),
                            principal_id: principal_id.clone(),
                            role_id: role_id.clone(),
                            scope_id: scope_id.clone(),
                        },
                    ));
                }
            }
            if options.prune_roles {
                for assignment in live {
                    let retained = desired.iter().any(|(_, principal_id, role_id)| {
                        eq_id(&assignment.principal_id, principal_id)
                            && eq_id(&assignment.role_id, role_id)
                    });
                    if !retained {
                        changes.push(change(
                            "roleAssignment",
                            Action::Delete,
                            format!(
                                "roleAssignment|{}",
                                assignment.id.rsplit('/').next().unwrap_or("unknown")
                            ),
                            "absent from specs at managed scope",
                            Target::RoleAssignment {
                                desired: desired[0].0.clone(),
                                principal_id: assignment.principal_id,
                                role_id: assignment.role_id,
                                scope_id: assignment.id,
                            },
                        ));
                    }
                }
            }
        }
    }
    if wanted(options, "dnsRecordSet") && !bundle.dns.is_empty() {
        let mut desired_by_zone: BTreeMap<&str, Vec<&DnsRecord>> = BTreeMap::new();
        for item in &bundle.dns {
            desired_by_zone.entry(&item.zone).or_default().push(item);
        }
        for (zone, desired) in desired_by_zone {
            let live = provider.list_dns(zone)?;
            let desired_keys: BTreeSet<_> = desired.iter().map(|item| item.key()).collect();
            for item in desired {
                match live.iter().find(|candidate| candidate.key() == item.key()) {
                    None => changes.push(change(
                        "dnsRecordSet",
                        Action::Create,
                        item.key(),
                        "missing",
                        Target::Dns(item.clone()),
                    )),
                    Some(current) if !current.same_data(item) => changes.push(change(
                        "dnsRecordSet",
                        Action::Update,
                        item.key(),
                        "TTL or values differ",
                        Target::Dns(item.clone()),
                    )),
                    Some(_) => {}
                }
            }
            if options.prune_dns {
                for item in live {
                    if !desired_keys.contains(&item.key()) && !dns_protected(&item) {
                        changes.push(change(
                            "dnsRecordSet",
                            Action::Delete,
                            item.key(),
                            "absent from specs",
                            Target::Dns(item),
                        ));
                    }
                }
            }
        }
    }
    Ok(changes)
}

fn eq_id(left: &str, right: &str) -> bool {
    left.eq_ignore_ascii_case(right)
        || left.rsplit('/').next().is_some_and(|tail| {
            tail.eq_ignore_ascii_case(right.rsplit('/').next().unwrap_or(right))
        })
}

fn same_role_definition(left: &RoleDefinition, right: &RoleDefinition, subscription: &str) -> bool {
    let scopes = |role: &RoleDefinition| -> Option<BTreeSet<String>> {
        if role.assignable_scopes.is_empty() {
            return Some(BTreeSet::from([format!("/subscriptions/{subscription}")]));
        }
        role.assignable_scopes
            .iter()
            .map(|scope| scope.arm_id(subscription).ok())
            .collect()
    };
    left.description == right.description
        && scopes(left) == scopes(right)
        && normalized(&left.actions) == normalized(&right.actions)
        && normalized(&left.not_actions) == normalized(&right.not_actions)
        && normalized(&left.data_actions) == normalized(&right.data_actions)
        && normalized(&left.not_data_actions) == normalized(&right.not_data_actions)
}
fn dns_protected(item: &DnsRecord) -> bool {
    item.name == "@" && matches!(item.record_type.to_ascii_uppercase().as_str(), "SOA" | "NS")
}
fn change(
    kind: &'static str,
    action: Action,
    key: String,
    summary: &str,
    target: Target,
) -> Change {
    Change {
        kind,
        action,
        key,
        summary: summary.into(),
        target,
    }
}

pub fn apply<P: Provider>(
    provider: &mut P,
    changes: &[Change],
    subscription: &str,
) -> Result<(), String> {
    for change in changes {
        match (&change.action, &change.target) {
            (Action::Delete, Target::Dns(item)) => provider.delete_dns(item)?,
            (_, Target::Dns(item)) => provider.put_dns(item)?,
            (Action::Delete, Target::Group(item)) => provider.delete_group(&item.name)?,
            (_, Target::Group(item)) => provider.put_group(item)?,
            (Action::Delete, Target::App(item)) => provider.delete_app(&item.name)?,
            (_, Target::App(item)) => provider.put_app(item)?,
            (Action::Delete, Target::RoleDefinition(item)) => {
                provider.delete_role_definition(&item.name, subscription)?
            }
            (_, Target::RoleDefinition(item)) => {
                provider.put_role_definition(item, subscription)?
            }
            (
                Action::Delete,
                Target::RoleAssignment {
                    principal_id,
                    role_id,
                    scope_id,
                    ..
                },
            ) => provider.delete_role_assignment(&LiveAssignment {
                id: scope_id.clone(),
                principal_id: principal_id.clone(),
                role_id: role_id.clone(),
                scope: String::new(),
            })?,
            (
                _,
                Target::RoleAssignment {
                    principal_id,
                    role_id,
                    scope_id,
                    ..
                },
            ) => provider.put_role_assignment(principal_id, role_id, scope_id)?,
            (Action::Delete, Target::ResourceGroup(item)) => {
                provider.delete_resource_group(&item.name)?
            }
            (_, Target::ResourceGroup(item)) => provider.put_resource_group(item)?,
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn apex_provider_records_are_protected() {
        let item = DnsRecord {
            zone: "example.com".into(),
            record_type: "NS".into(),
            name: "@".into(),
            ttl: 60,
            values: vec!["ns.example.com".into()],
        };
        assert!(dns_protected(&item));
    }

    #[derive(Default)]
    struct AssignmentProvider {
        assignments: Vec<LiveAssignment>,
        dns: Vec<DnsRecord>,
        groups: Vec<SecurityGroup>,
        group_names: Vec<String>,
        apps: Vec<AppRegistration>,
        app_names: Vec<String>,
        role_definitions: Vec<RoleDefinition>,
        resource_groups: Vec<ResourceGroup>,
    }

    impl Provider for AssignmentProvider {
        fn ensure_zone(&mut self, _: &str) -> Result<(), String> {
            Ok(())
        }
        fn list_dns(&mut self, _: &str) -> Result<Vec<DnsRecord>, String> {
            Ok(self.dns.clone())
        }
        fn put_dns(&mut self, _: &DnsRecord) -> Result<(), String> {
            Ok(())
        }
        fn delete_dns(&mut self, _: &DnsRecord) -> Result<(), String> {
            Ok(())
        }
        fn get_group(&mut self, name: &str) -> Result<Option<SecurityGroup>, String> {
            Ok(self.groups.iter().find(|item| item.name == name).cloned())
        }
        fn list_group_names(&mut self) -> Result<Vec<String>, String> {
            Ok(self.group_names.clone())
        }
        fn put_group(&mut self, _: &SecurityGroup) -> Result<(), String> {
            Ok(())
        }
        fn delete_group(&mut self, _: &str) -> Result<(), String> {
            Ok(())
        }
        fn get_app(&mut self, name: &str) -> Result<Option<AppRegistration>, String> {
            Ok(self.apps.iter().find(|item| item.name == name).cloned())
        }
        fn list_app_names(&mut self) -> Result<Vec<String>, String> {
            Ok(self.app_names.clone())
        }
        fn put_app(&mut self, _: &AppRegistration) -> Result<(), String> {
            Ok(())
        }
        fn delete_app(&mut self, _: &str) -> Result<(), String> {
            Ok(())
        }
        fn list_role_definitions(&mut self, _: &str) -> Result<Vec<RoleDefinition>, String> {
            Ok(self.role_definitions.clone())
        }
        fn put_role_definition(&mut self, _: &RoleDefinition, _: &str) -> Result<(), String> {
            Ok(())
        }
        fn delete_role_definition(&mut self, _: &str, _: &str) -> Result<(), String> {
            Ok(())
        }
        fn resolve_principal(&mut self, name: &str, _: &str) -> Result<String, String> {
            Ok(format!("id-{name}"))
        }
        fn resolve_role(&mut self, name: &str, _: &str) -> Result<String, String> {
            Ok(format!("role-{name}"))
        }
        fn list_role_assignments(&mut self, _: &str) -> Result<Vec<LiveAssignment>, String> {
            Ok(self.assignments.clone())
        }
        fn put_role_assignment(&mut self, _: &str, _: &str, _: &str) -> Result<(), String> {
            Ok(())
        }
        fn delete_role_assignment(&mut self, _: &LiveAssignment) -> Result<(), String> {
            Ok(())
        }
        fn list_resource_groups(&mut self) -> Result<Vec<ResourceGroup>, String> {
            Ok(self.resource_groups.clone())
        }
        fn put_resource_group(&mut self, _: &ResourceGroup) -> Result<(), String> {
            Ok(())
        }
        fn delete_resource_group(&mut self, _: &str) -> Result<(), String> {
            Ok(())
        }
    }

    #[test]
    fn prune_retains_all_desired_assignments_at_shared_scope() {
        let scope = Scope {
            subscription: "00000000-0000-0000-0000-000000000001".into(),
            ..Scope::default()
        };
        let bundle = Bundle {
            role_assignments: vec![
                RoleAssignment {
                    principal: "one".into(),
                    principal_type: "group".into(),
                    role: "reader".into(),
                    scope: scope.clone(),
                },
                RoleAssignment {
                    principal: "two".into(),
                    principal_type: "group".into(),
                    role: "reader".into(),
                    scope: scope.clone(),
                },
            ],
            ..Bundle::default()
        };
        let mut provider = AssignmentProvider {
            assignments: vec![
                LiveAssignment {
                    id: "assignment-one".into(),
                    principal_id: "id-one".into(),
                    role_id: "role-reader".into(),
                    scope: scope.arm_id("").unwrap(),
                },
                LiveAssignment {
                    id: "assignment-two".into(),
                    principal_id: "id-two".into(),
                    role_id: "role-reader".into(),
                    scope: scope.arm_id("").unwrap(),
                },
            ],
            ..AssignmentProvider::default()
        };
        let changes = plan(
            &mut provider,
            &bundle,
            &Options {
                subscription: scope.subscription.clone(),
                prune_roles: true,
                ..Options::default()
            },
        )
        .unwrap();
        assert!(changes.is_empty());
    }

    fn scope(subscription: &str) -> Scope {
        Scope {
            subscription: subscription.into(),
            ..Scope::default()
        }
    }

    fn role(name: &str, scopes: Vec<Scope>) -> RoleDefinition {
        RoleDefinition {
            name: name.into(),
            description: "synthetic role".into(),
            assignable_scopes: scopes,
            actions: vec!["Microsoft.Network/dnsZones/read".into()],
            not_actions: vec![],
            data_actions: vec![],
            not_data_actions: vec![],
        }
    }

    fn resource_group(name: &str, tags: &[(&str, &str)]) -> ResourceGroup {
        ResourceGroup {
            name: name.into(),
            location: "eastus".into(),
            tags: tags
                .iter()
                .map(|(key, value)| ((*key).into(), (*value).into()))
                .collect(),
        }
    }

    #[test]
    fn plans_every_kind_in_legacy_compatible_order() {
        let subscription = "00000000-0000-0000-0000-000000000001";
        let bundle = Bundle {
            resource_groups: vec![resource_group("resources", &[])],
            groups: vec![SecurityGroup {
                name: "readers".into(),
                owners: vec![],
                members: vec![],
            }],
            apps: vec![AppRegistration {
                name: "application".into(),
                owners: vec![],
                service_principal: true,
            }],
            role_definitions: vec![role("reader", vec![scope(subscription)])],
            role_assignments: vec![RoleAssignment {
                principal: "readers".into(),
                principal_type: "group".into(),
                role: "reader".into(),
                scope: scope(subscription),
            }],
            dns: vec![DnsRecord {
                zone: "example.com".into(),
                record_type: "A".into(),
                name: "docs".into(),
                ttl: 300,
                values: vec!["192.0.2.10".into()],
            }],
        };
        let changes = plan(
            &mut AssignmentProvider::default(),
            &bundle,
            &Options {
                subscription: subscription.into(),
                ..Options::default()
            },
        )
        .unwrap();
        assert_eq!(
            changes.iter().map(|change| change.kind).collect::<Vec<_>>(),
            [
                "resourceGroup",
                "securityGroup",
                "appRegistration",
                "roleDefinition",
                "roleAssignment",
                "dnsRecordSet",
            ]
        );
    }

    #[test]
    fn identity_and_resource_group_pruning_respects_policy() {
        let bundle = Bundle {
            groups: vec![SecurityGroup {
                name: "wanted-group".into(),
                owners: vec![],
                members: vec![],
            }],
            apps: vec![AppRegistration {
                name: "wanted-app".into(),
                owners: vec![],
                service_principal: false,
            }],
            resource_groups: vec![resource_group("wanted-resources", &[])],
            ..Bundle::default()
        };
        let provider = || AssignmentProvider {
            group_names: vec!["wanted-group".into(), "stale-group".into()],
            app_names: vec!["wanted-app".into(), "stale-app".into()],
            resource_groups: vec![
                resource_group("wanted-resources", &[]),
                resource_group("stale-resources", &[]),
            ],
            ..AssignmentProvider::default()
        };

        let changes = plan(&mut provider(), &bundle, &Options::default()).unwrap();
        assert!(!changes.iter().any(|change| change.action == Action::Delete));

        let changes = plan(
            &mut provider(),
            &bundle,
            &Options {
                prune_identities: true,
                prune_resource_groups: true,
                ..Options::default()
            },
        )
        .unwrap();
        let deleted: Vec<_> = changes
            .iter()
            .filter(|change| change.action == Action::Delete)
            .map(|change| change.key.as_str())
            .collect();
        assert_eq!(
            deleted,
            [
                "resourceGroup|stale-resources",
                "securityGroup|stale-group",
                "appRegistration|stale-app",
            ]
        );
    }

    #[test]
    fn role_definition_scope_comparison_detects_only_real_drift() {
        let subscription = "00000000-0000-0000-0000-000000000001";
        let resource_scope = Scope {
            subscription: subscription.into(),
            resource_group: "resources".into(),
            ..Scope::default()
        };
        let desired = role("reader", vec![scope(subscription), resource_scope.clone()]);
        let bundle = Bundle {
            role_definitions: vec![desired.clone()],
            ..Bundle::default()
        };
        let mut drifted = AssignmentProvider {
            role_definitions: vec![role(
                "reader",
                vec![scope("00000000-0000-0000-0000-000000000002")],
            )],
            ..AssignmentProvider::default()
        };
        let changes = plan(
            &mut drifted,
            &bundle,
            &Options {
                subscription: subscription.into(),
                ..Options::default()
            },
        )
        .unwrap();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].action, Action::Update);

        let mut equivalent = AssignmentProvider {
            role_definitions: vec![role(
                "reader",
                vec![resource_scope.clone(), scope(subscription), resource_scope],
            )],
            ..AssignmentProvider::default()
        };
        assert!(
            plan(
                &mut equivalent,
                &bundle,
                &Options {
                    subscription: subscription.into(),
                    ..Options::default()
                }
            )
            .unwrap()
            .is_empty()
        );
    }

    #[test]
    fn resource_group_updates_merge_declared_tags_only_when_needed() {
        let bundle = Bundle {
            resource_groups: vec![resource_group("resources", &[("managed", "new")])],
            ..Bundle::default()
        };
        let mut provider = AssignmentProvider {
            resource_groups: vec![resource_group(
                "resources",
                &[("managed", "old"), ("preserved", "live")],
            )],
            ..AssignmentProvider::default()
        };
        let changes = plan(&mut provider, &bundle, &Options::default()).unwrap();
        assert_eq!(changes.len(), 1);
        let Target::ResourceGroup(updated) = &changes[0].target else {
            panic!("expected resource-group update")
        };
        assert_eq!(updated.tags.get("managed").map(String::as_str), Some("new"));
        assert_eq!(
            updated.tags.get("preserved").map(String::as_str),
            Some("live")
        );

        let mut satisfied = AssignmentProvider {
            resource_groups: vec![resource_group(
                "resources",
                &[("managed", "new"), ("preserved", "live")],
            )],
            ..AssignmentProvider::default()
        };
        assert!(
            plan(&mut satisfied, &bundle, &Options::default())
                .unwrap()
                .is_empty()
        );
    }
}
