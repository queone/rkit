//! Strict provider-neutral YAML specification loading.

use super::model::*;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use yaml_rust2::{Yaml, YamlLoader};

type Hash = yaml_rust2::yaml::Hash;

pub fn load(directory: &Path) -> Result<Bundle, String> {
    let mut paths = Vec::new();
    collect(directory, &mut paths)?;
    paths.sort();
    let mut bundle = Bundle::default();
    let mut keys = BTreeSet::new();
    for path in paths {
        let raw = fs::read_to_string(&path).map_err(|error| format!("read spec: {error}"))?;
        let docs = YamlLoader::load_from_str(&raw)
            .map_err(|error| format!("parse {}: {error}", path.display()))?;
        if docs.len() != 1 {
            return Err("each spec file must contain exactly one YAML document".into());
        }
        let map = mapping(&docs[0], "spec")?;
        let kind = required_string(map, "kind")?;
        let resource_key = match kind.as_str() {
            "dnsRecordSet" => {
                reject_unknown(map, &["kind", "zone", "type", "name", "ttl", "values"])?;
                let item = DnsRecord {
                    zone: required_string(map, "zone")?,
                    record_type: required_string(map, "type")?,
                    name: required_string(map, "name")?,
                    ttl: required_i64(map, "ttl")?,
                    values: required_strings(map, "values")?,
                };
                if item.values.is_empty() {
                    return Err("dnsRecordSet values must not be empty".into());
                }
                let key = item.key();
                bundle.dns.push(item);
                key
            }
            "securityGroup" => {
                reject_unknown(map, &["kind", "name", "owners", "members"])?;
                let item = SecurityGroup {
                    name: required_string(map, "name")?,
                    owners: optional_strings(map, "owners")?,
                    members: optional_strings(map, "members")?,
                };
                let key = format!("securityGroup|{}", item.name);
                bundle.groups.push(item);
                key
            }
            "appRegistration" => {
                reject_unknown(map, &["kind", "name", "owners", "servicePrincipal"])?;
                let item = AppRegistration {
                    name: required_string(map, "name")?,
                    owners: optional_strings(map, "owners")?,
                    service_principal: optional_bool(map, "servicePrincipal")?.unwrap_or(false),
                };
                let key = format!("appRegistration|{}", item.name);
                bundle.apps.push(item);
                key
            }
            "roleDefinition" => {
                reject_unknown(
                    map,
                    &[
                        "kind",
                        "name",
                        "description",
                        "assignableScopes",
                        "actions",
                        "notActions",
                        "dataActions",
                        "notDataActions",
                    ],
                )?;
                let item = RoleDefinition {
                    name: required_string(map, "name")?,
                    description: optional_string(map, "description")?,
                    assignable_scopes: optional_scopes(map, "assignableScopes")?,
                    actions: optional_strings(map, "actions")?,
                    not_actions: optional_strings(map, "notActions")?,
                    data_actions: optional_strings(map, "dataActions")?,
                    not_data_actions: optional_strings(map, "notDataActions")?,
                };
                let key = format!("roleDefinition|{}", item.name);
                bundle.role_definitions.push(item);
                key
            }
            "roleAssignment" => {
                reject_unknown(
                    map,
                    &["kind", "principal", "principalType", "role", "scope"],
                )?;
                let item = RoleAssignment {
                    principal: required_string(map, "principal")?,
                    principal_type: required_string(map, "principalType")?,
                    role: required_string(map, "role")?,
                    scope: required_scope(map, "scope")?,
                };
                let key = format!(
                    "roleAssignment|{}|{}|{:?}",
                    item.principal, item.role, item.scope
                );
                bundle.role_assignments.push(item);
                key
            }
            "resourceGroup" => {
                reject_unknown(map, &["kind", "name", "location", "tags"])?;
                let item = ResourceGroup {
                    name: required_string(map, "name")?,
                    location: required_string(map, "location")?,
                    tags: optional_string_map(map, "tags")?,
                };
                let key = format!("resourceGroup|{}", item.name);
                bundle.resource_groups.push(item);
                key
            }
            _ => return Err(format!("unknown kind {kind:?}")),
        };
        if !keys.insert(resource_key.clone()) {
            return Err(format!("duplicate resource key {resource_key:?}"));
        }
    }
    Ok(bundle)
}

fn collect(directory: &Path, paths: &mut Vec<PathBuf>) -> Result<(), String> {
    for entry in fs::read_dir(directory).map_err(|error| format!("read spec directory: {error}"))? {
        let entry = entry.map_err(|error| format!("read spec directory entry: {error}"))?;
        let file_type = entry
            .file_type()
            .map_err(|error| format!("inspect spec path: {error}"))?;
        if file_type.is_symlink() {
            continue;
        }
        let path = entry.path();
        if file_type.is_dir() {
            collect(&path, paths)?;
        } else if file_type.is_file()
            && matches!(
                path.extension()
                    .and_then(|value| value.to_str())
                    .map(str::to_ascii_lowercase)
                    .as_deref(),
                Some("yaml" | "yml")
            )
        {
            paths.push(path);
        }
    }
    Ok(())
}

fn key(name: &str) -> Yaml {
    Yaml::String(name.into())
}
fn mapping<'a>(value: &'a Yaml, context: &str) -> Result<&'a Hash, String> {
    value
        .as_hash()
        .ok_or_else(|| format!("{context} must be a mapping"))
}
fn reject_unknown(map: &Hash, allowed: &[&str]) -> Result<(), String> {
    for candidate in map.keys() {
        let name = candidate.as_str().ok_or("spec keys must be strings")?;
        if !allowed.contains(&name) {
            return Err(format!("unknown field {name:?}"));
        }
    }
    Ok(())
}
fn required_string(map: &Hash, name: &str) -> Result<String, String> {
    let value = optional_string(map, name)?;
    if value.is_empty() {
        Err(format!("{name} is required"))
    } else {
        Ok(value)
    }
}
fn optional_string(map: &Hash, name: &str) -> Result<String, String> {
    match map.get(&key(name)) {
        None | Some(Yaml::Null) => Ok(String::new()),
        Some(Yaml::String(value)) => Ok(value.clone()),
        _ => Err(format!("{name} must be a string")),
    }
}
fn required_i64(map: &Hash, name: &str) -> Result<i64, String> {
    match map.get(&key(name)) {
        Some(Yaml::Integer(value)) => Ok(*value),
        _ => Err(format!("{name} must be an integer")),
    }
}
fn optional_bool(map: &Hash, name: &str) -> Result<Option<bool>, String> {
    match map.get(&key(name)) {
        None | Some(Yaml::Null) => Ok(None),
        Some(Yaml::Boolean(value)) => Ok(Some(*value)),
        _ => Err(format!("{name} must be a boolean")),
    }
}
fn required_strings(map: &Hash, name: &str) -> Result<Vec<String>, String> {
    if !map.contains_key(&key(name)) {
        return Err(format!("{name} is required"));
    }
    optional_strings(map, name)
}
fn optional_strings(map: &Hash, name: &str) -> Result<Vec<String>, String> {
    match map.get(&key(name)) {
        None | Some(Yaml::Null) => Ok(Vec::new()),
        Some(Yaml::Array(values)) => values
            .iter()
            .map(|value| {
                value
                    .as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| format!("{name} entries must be strings"))
            })
            .collect(),
        _ => Err(format!("{name} must be a list")),
    }
}
fn optional_string_map(map: &Hash, name: &str) -> Result<BTreeMap<String, String>, String> {
    match map.get(&key(name)) {
        None | Some(Yaml::Null) => Ok(BTreeMap::new()),
        Some(value) => mapping(value, name)?
            .iter()
            .map(|(key, value)| {
                Ok((
                    key.as_str()
                        .ok_or_else(|| format!("{name} keys must be strings"))?
                        .to_owned(),
                    value
                        .as_str()
                        .ok_or_else(|| format!("{name} values must be strings"))?
                        .to_owned(),
                ))
            })
            .collect(),
    }
}
fn required_scope(map: &Hash, name: &str) -> Result<Scope, String> {
    let value = map
        .get(&key(name))
        .ok_or_else(|| format!("{name} is required"))?;
    parse_scope(value)
}
fn optional_scopes(map: &Hash, name: &str) -> Result<Vec<Scope>, String> {
    match map.get(&key(name)) {
        None | Some(Yaml::Null) => Ok(Vec::new()),
        Some(Yaml::Array(values)) => values.iter().map(parse_scope).collect(),
        _ => Err(format!("{name} must be a list")),
    }
}
fn parse_scope(value: &Yaml) -> Result<Scope, String> {
    let map = mapping(value, "scope")?;
    reject_unknown(
        map,
        &[
            "managementGroup",
            "subscription",
            "resourceGroup",
            "dnsZone",
        ],
    )?;
    Ok(Scope {
        management_group: optional_string(map, "managementGroup")?,
        subscription: optional_string(map, "subscription")?,
        resource_group: optional_string(map, "resourceGroup")?,
        dns_zone: optional_string(map, "dnsZone")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fixture_loads_every_kind() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/attune/specs");
        let bundle = load(&root).unwrap();
        assert_eq!(bundle.len(), 6);
    }
}
