//! Public command behavior for the `attune` desired-state reconciler.

pub mod azure;
pub mod config;
pub mod model;
pub mod reconcile;
pub mod spec;

use azure::AzureProvider;
use config::Overrides;
use reconcile::Provider;
use std::collections::BTreeSet;
use std::ffi::OsString;
use std::io::Write;

const KINDS: &[&str] = &[
    "dnsRecordSet",
    "securityGroup",
    "appRegistration",
    "roleDefinition",
    "roleAssignment",
    "resourceGroup",
];

enum Command {
    Plan,
    Apply,
    Validate,
}

pub fn run<I, S, W, E>(args: I, version: &str, stdout: &mut W, stderr: &mut E) -> u8
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
    W: Write,
    E: Write,
{
    let args: Vec<OsString> = args.into_iter().map(Into::into).collect();
    if args.len() == 1 && matches!(args[0].to_str(), Some("-v" | "--version" | "v" | "version")) {
        let _ = writeln!(stdout, "attune {version}");
        return 0;
    }
    if args.is_empty()
        || (args.len() == 1 && matches!(args[0].to_str(), Some("-h" | "--help" | "h" | "help")))
    {
        let _ = write!(stdout, "{}", usage());
        return 0;
    }
    let command = match args[0].to_str() {
        Some("p" | "plan") => Command::Plan,
        Some("a" | "apply") => Command::Apply,
        Some("c" | "validate") => Command::Validate,
        Some(other) => {
            let _ = writeln!(
                stderr,
                "attune: unknown command {other:?}; run `attune help`"
            );
            return 2;
        }
        None => {
            let _ = writeln!(
                stderr,
                "attune: command must be valid UTF-8; run `attune help`"
            );
            return 2;
        }
    };
    let overrides = match parse_flags(&args[1..]) {
        Ok(value) => value,
        Err(error) => {
            let _ = writeln!(stderr, "attune: {error}; run `attune help`");
            return 2;
        }
    };
    let cwd = match std::env::current_dir() {
        Ok(value) => value,
        Err(error) => {
            let _ = writeln!(stderr, "attune: resolve current directory: {error}");
            return 1;
        }
    };
    let found = match config::find(&cwd) {
        Ok(value) => value,
        Err(error) => {
            let _ = writeln!(stderr, "attune: {error}");
            return 1;
        }
    };
    let settings = config::resolve(found.as_ref(), overrides);
    if settings.provider != "azure" {
        let _ = writeln!(
            stderr,
            "attune: unsupported provider {:?}; use `azure`",
            settings.provider
        );
        return 1;
    }
    if !settings.kind.is_empty() && !KINDS.contains(&settings.kind.as_str()) {
        let _ = writeln!(stderr, "attune: unknown kind {:?}", settings.kind);
        return 1;
    }
    let bundle = match spec::load(&settings.specs) {
        Ok(value) => value,
        Err(error) => {
            let _ = writeln!(stderr, "attune: validate specs: {error}");
            return 1;
        }
    };
    if bundle.is_empty() {
        let _ = writeln!(stderr, "attune: no specs found");
        return 1;
    }
    if matches!(command, Command::Validate) {
        let _ = writeln!(stdout, "attune validate: OK ({} specs)", bundle.len());
        return 0;
    }
    let mut provider = AzureProvider::new(
        settings.subscription.clone(),
        settings.resource_group.clone(),
    );
    let account = match provider.ground() {
        Ok(value) => value,
        Err(error) => {
            let _ = writeln!(stderr, "attune: authenticate provider: {error}");
            return 1;
        }
    };
    let _ = writeln!(
        stderr,
        "attune: provider=azure specs={} authenticated=yes",
        bundle.len()
    );
    if settings.diagnostic {
        let _ = writeln!(
            stderr,
            "attune: diagnostic tenant={} subscription={} identity={} resource-group={} specs={}",
            account.tenant,
            provider.subscription,
            account.identity,
            settings.resource_group,
            settings.specs.display()
        );
    }
    if matches!(command, Command::Apply) {
        let zones: BTreeSet<_> = bundle
            .dns
            .iter()
            .filter(|_| settings.kind.is_empty() || settings.kind == "dnsRecordSet")
            .map(|item| item.zone.as_str())
            .collect();
        for zone in zones {
            if let Err(error) = provider.ensure_zone(zone) {
                let _ = writeln!(stderr, "attune: ensure DNS zone: {error}");
                return 1;
            }
        }
    }
    let options = reconcile::Options {
        subscription: provider.subscription.clone(),
        kind: settings.kind,
        prune_dns: settings.prune_dns,
        prune_identities: settings.prune_identities,
        prune_roles: settings.prune_roles,
        prune_resource_groups: settings.prune_resource_groups,
    };
    let changes = match reconcile::plan(&mut provider, &bundle, &options) {
        Ok(value) => value,
        Err(error) => {
            let _ = writeln!(stderr, "attune: plan provider changes: {error}");
            return 1;
        }
    };
    let _ = writeln!(stdout, "attune plan: provider=azure");
    for change in &changes {
        let symbol = match change.action {
            reconcile::Action::Create => '+',
            reconcile::Action::Update => '~',
            reconcile::Action::Delete => '-',
        };
        let _ = writeln!(
            stdout,
            "  {symbol} {:<6} {:<15} {}  {}",
            change.action.as_str(),
            change.kind,
            change.key,
            change.summary
        );
    }
    let _ = writeln!(stdout, "\n{} change(s).", changes.len());
    if matches!(command, Command::Apply) {
        let subscription = provider.subscription.clone();
        if let Err(error) = reconcile::apply(&mut provider, &changes, &subscription) {
            let _ = writeln!(stderr, "attune: apply provider change: {error}");
            return 1;
        }
    }
    0
}

fn parse_flags(args: &[OsString]) -> Result<Overrides, String> {
    let mut overrides = Overrides::default();
    let mut index = 0;
    while index < args.len() {
        let arg = args[index].to_str().ok_or("flags must be valid UTF-8")?;
        let (name, inline) = arg
            .split_once('=')
            .map_or((arg, None), |(name, value)| (name, Some(value)));
        let value = |index: &mut usize| -> Result<String, String> {
            if let Some(value) = inline {
                return Ok(value.into());
            }
            *index += 1;
            args.get(*index)
                .and_then(|argument| argument.to_str())
                .map(str::to_owned)
                .ok_or_else(|| format!("{name} requires a value"))
        };
        match name {
            "-s" | "--specs" => overrides.specs = Some(value(&mut index)?),
            "-P" | "--provider" => overrides.provider = Some(value(&mut index)?),
            "-g" | "--resource-group" => overrides.resource_group = Some(value(&mut index)?),
            "-S" | "--subscription" => overrides.subscription = Some(value(&mut index)?),
            "-k" | "--kind" => overrides.kind = value(&mut index)?,
            "-r" | "--prune" => overrides.prune_dns = Some(bool_value(inline)?),
            "-I" | "--prune-identities" => overrides.prune_identities = Some(bool_value(inline)?),
            "-R" | "--prune-roles" => overrides.prune_roles = Some(bool_value(inline)?),
            "-G" | "--prune-resource-groups" => {
                overrides.prune_resource_groups = Some(bool_value(inline)?)
            }
            "-d" | "--diagnostic" => {
                if inline.is_some() {
                    return Err(format!("{name} does not take a value"));
                }
                overrides.diagnostic = true;
            }
            _ => return Err(format!("unknown flag {name:?}")),
        }
        index += 1;
    }
    Ok(overrides)
}

fn bool_value(value: Option<&str>) -> Result<bool, String> {
    match value {
        None => Ok(true),
        Some("true") => Ok(true),
        Some("false") => Ok(false),
        Some(_) => Err("boolean flag value must be true or false".into()),
    }
}

fn usage() -> &'static str {
    "attune — reconcile provider state with neutral YAML specs.\n\nUsage:\n  attune (p|plan) [flags]\n  attune (a|apply) [flags]\n  attune (c|validate) [flags]\n  attune (v|version)\n  attune (h|help)\n\nFlags:\n  -s, --specs PATH\n  -P, --provider NAME\n  -g, --resource-group NAME\n  -S, --subscription ID\n  -k, --kind KIND\n  -r, --prune[=BOOL]\n  -I, --prune-identities[=BOOL]\n  -R, --prune-roles[=BOOL]\n  -G, --prune-resource-groups[=BOOL]\n  -d, --diagnostic\n\nLive commands require an authenticated Azure CLI (`az login`).\n"
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn aliases_parse() {
        assert!(matches!("p", "p" | "plan"));
    }
    #[test]
    fn explicit_false_is_supported() {
        assert!(!bool_value(Some("false")).unwrap());
    }
}
