//! `attune.yaml` discovery and precedence.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use yaml_rust2::{Yaml, YamlLoader};

#[derive(Clone, Debug, Default)]
pub struct Config {
    pub provider: String,
    pub specs: String,
    pub subscription: String,
    pub resource_group: String,
    pub prune_dns: Option<bool>,
    pub prune_identities: Option<bool>,
    pub prune_roles: Option<bool>,
    pub prune_resource_groups: Option<bool>,
    pub directory: PathBuf,
}

#[derive(Clone, Debug, Default)]
pub struct Overrides {
    pub provider: Option<String>,
    pub specs: Option<String>,
    pub subscription: Option<String>,
    pub resource_group: Option<String>,
    pub prune_dns: Option<bool>,
    pub prune_identities: Option<bool>,
    pub prune_roles: Option<bool>,
    pub prune_resource_groups: Option<bool>,
    pub kind: String,
    pub diagnostic: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Settings {
    pub provider: String,
    pub specs: PathBuf,
    pub subscription: String,
    pub resource_group: String,
    pub prune_dns: bool,
    pub prune_identities: bool,
    pub prune_roles: bool,
    pub prune_resource_groups: bool,
    pub kind: String,
    pub diagnostic: bool,
}

pub fn find(start: &Path) -> Result<Option<Config>, String> {
    let mut directory = start
        .canonicalize()
        .map_err(|error| format!("resolve configuration start directory: {error}"))?;
    loop {
        let candidate = directory.join("attune.yaml");
        if candidate.is_file() {
            return load(&candidate).map(Some);
        }
        if !directory.pop() {
            return Ok(None);
        }
    }
}

fn expand_environment(input: &str) -> Result<String, String> {
    let mut output = String::new();
    let bytes = input.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if let Some(offset) = input[index..].find('$') {
            let dollar = index + offset;
            output.push_str(&input[index..dollar]);
            index = dollar;
            let (name, consumed) = if bytes.get(index + 1) == Some(&b'{') {
                let end = input[index + 2..].find('}').ok_or_else(|| {
                    "unterminated environment reference in attune.yaml".to_string()
                })? + index
                    + 2;
                (&input[index + 2..end], end - index + 1)
            } else {
                let end = input[index + 1..]
                    .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                    .map_or(input.len(), |offset| index + 1 + offset);
                (&input[index + 1..end], end - index)
            };
            if name.is_empty() {
                output.push('$');
                index += 1;
                continue;
            }
            output.push_str(&env::var(name).unwrap_or_default());
            index += consumed;
        } else {
            output.push_str(&input[index..]);
            break;
        }
    }
    Ok(output)
}

pub fn load(path: &Path) -> Result<Config, String> {
    let raw = fs::read_to_string(path).map_err(|error| format!("read attune.yaml: {error}"))?;
    let expanded = expand_environment(&raw)?;
    let docs = YamlLoader::load_from_str(&expanded)
        .map_err(|error| format!("parse attune.yaml: {error}"))?;
    let root = docs.first().ok_or("attune.yaml is empty")?;
    let map = mapping(root, "attune.yaml")?;
    reject_unknown(map, &["provider", "specs", "azure", "prune"], "attune.yaml")?;
    let azure = optional_mapping(map, "azure")?;
    reject_unknown(
        azure,
        &["subscription", "resource_group"],
        "attune.yaml azure",
    )?;
    let prune = optional_mapping(map, "prune")?;
    reject_unknown(
        prune,
        &["dns", "identities", "roles", "resource_groups"],
        "attune.yaml prune",
    )?;
    Ok(Config {
        provider: optional_string(map, "provider")?,
        specs: optional_string(map, "specs")?,
        subscription: optional_string(azure, "subscription")?,
        resource_group: optional_string(azure, "resource_group")?,
        prune_dns: optional_bool(prune, "dns")?,
        prune_identities: optional_bool(prune, "identities")?,
        prune_roles: optional_bool(prune, "roles")?,
        prune_resource_groups: optional_bool(prune, "resource_groups")?,
        directory: path.parent().unwrap_or(Path::new(".")).to_path_buf(),
    })
}

pub fn resolve(config: Option<&Config>, overrides: Overrides) -> Settings {
    let mut settings = Settings {
        provider: "azure".into(),
        specs: PathBuf::from("dns"),
        subscription: String::new(),
        resource_group: String::new(),
        prune_dns: true,
        prune_identities: false,
        prune_roles: false,
        prune_resource_groups: false,
        kind: overrides.kind.clone(),
        diagnostic: overrides.diagnostic,
    };
    if let Some(config) = config {
        if !config.provider.is_empty() {
            settings.provider.clone_from(&config.provider);
        }
        if !config.specs.is_empty() {
            settings.specs = PathBuf::from(&config.specs);
            if settings.specs.is_relative() {
                settings.specs = config.directory.join(&settings.specs);
            }
        }
        if !config.subscription.is_empty() {
            settings.subscription.clone_from(&config.subscription);
        }
        if !config.resource_group.is_empty() {
            settings.resource_group.clone_from(&config.resource_group);
        }
        if let Some(value) = config.prune_dns {
            settings.prune_dns = value;
        }
        if let Some(value) = config.prune_identities {
            settings.prune_identities = value;
        }
        if let Some(value) = config.prune_roles {
            settings.prune_roles = value;
        }
        if let Some(value) = config.prune_resource_groups {
            settings.prune_resource_groups = value;
        }
    }
    if let Ok(value) = env::var("ARM_SUBSCRIPTION_ID")
        && !value.is_empty()
    {
        settings.subscription = value;
    }
    if let Ok(value) = env::var("ARM_RESOURCE_GROUP")
        && !value.is_empty()
    {
        settings.resource_group = value;
    }
    if let Some(value) = overrides.provider {
        settings.provider = value;
    }
    if let Some(value) = overrides.specs {
        settings.specs = PathBuf::from(value);
    }
    if let Some(value) = overrides.subscription {
        settings.subscription = value;
    }
    if let Some(value) = overrides.resource_group {
        settings.resource_group = value;
    }
    if let Some(value) = overrides.prune_dns {
        settings.prune_dns = value;
    }
    if let Some(value) = overrides.prune_identities {
        settings.prune_identities = value;
    }
    if let Some(value) = overrides.prune_roles {
        settings.prune_roles = value;
    }
    if let Some(value) = overrides.prune_resource_groups {
        settings.prune_resource_groups = value;
    }
    settings
}

type Hash = yaml_rust2::yaml::Hash;
fn key(name: &str) -> Yaml {
    Yaml::String(name.into())
}
fn mapping<'a>(value: &'a Yaml, context: &str) -> Result<&'a Hash, String> {
    value
        .as_hash()
        .ok_or_else(|| format!("{context} must be a mapping"))
}
fn optional_mapping<'a>(map: &'a Hash, name: &str) -> Result<&'a Hash, String> {
    static EMPTY: std::sync::OnceLock<Hash> = std::sync::OnceLock::new();
    match map.get(&key(name)) {
        None | Some(Yaml::Null) => Ok(EMPTY.get_or_init(Hash::new)),
        Some(value) => mapping(value, name),
    }
}
fn reject_unknown(map: &Hash, allowed: &[&str], context: &str) -> Result<(), String> {
    for candidate in map.keys() {
        let name = candidate
            .as_str()
            .ok_or_else(|| format!("{context} keys must be strings"))?;
        if !allowed.contains(&name) {
            return Err(format!("{context}: unknown field {name:?}"));
        }
    }
    Ok(())
}
fn optional_string(map: &Hash, name: &str) -> Result<String, String> {
    match map.get(&key(name)) {
        None | Some(Yaml::Null) => Ok(String::new()),
        Some(Yaml::String(value)) => Ok(value.clone()),
        _ => Err(format!("{name} must be a string")),
    }
}
fn optional_bool(map: &Hash, name: &str) -> Result<Option<bool>, String> {
    match map.get(&key(name)) {
        None | Some(Yaml::Null) => Ok(None),
        Some(Yaml::Boolean(value)) => Ok(Some(*value)),
        _ => Err(format!("{name} must be a boolean")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn defaults_are_safe() {
        let settings = resolve(None, Overrides::default());
        assert!(settings.prune_dns);
        assert!(!settings.prune_identities);
        assert_eq!(settings.provider, "azure");
    }
    #[test]
    fn expansion_preserves_unicode_text() {
        assert_eq!(
            expand_environment("specs: café\n").unwrap(),
            "specs: café\n"
        );
    }
}
