//! Azure CLI authentication plus ARM and Microsoft Graph REST providers.

use super::model::*;
use super::reconcile::{LiveAssignment, Provider};
use super::spec::is_directory_object_id;
use crate::pman::{HttpRequest, HttpTransport, TcpHttpTransport};
use openssl::sha::sha1;
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::process::Command;

const ARM: &str = "https://management.azure.com";
const GRAPH: &str = "https://graph.microsoft.com/v1.0";
const DNS_API: &str = "2018-05-01";
const GROUP_API: &str = "2021-04-01";
const ROLE_API: &str = "2022-04-01";

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Account {
    pub tenant: String,
    pub subscription: String,
    pub identity: String,
}

pub trait AzureCli {
    fn account(&self) -> Result<Account, String>;
    fn token(&self, resource: &str) -> Result<String, String>;
}

pub struct CommandAzureCli;

impl AzureCli for CommandAzureCli {
    fn account(&self) -> Result<Account, String> {
        let output = Command::new("az")
            .args(["account", "show", "--output", "json"])
            .output()
            .map_err(|error| {
                format!("run Azure CLI account lookup: {error}; run `az login` and retry")
            })?;
        if !output.status.success() {
            return Err("Azure CLI account lookup failed; run `az login` and retry".into());
        }
        let value: Value = serde_json::from_slice(&output.stdout)
            .map_err(|_| "Azure CLI returned malformed account data".to_string())?;
        Ok(Account {
            tenant: string(&value, "tenantId"),
            subscription: string(&value, "id"),
            identity: value
                .pointer("/user/name")
                .and_then(Value::as_str)
                .unwrap_or("authenticated")
                .to_owned(),
        })
    }

    fn token(&self, resource: &str) -> Result<String, String> {
        let output = Command::new("az")
            .args([
                "account",
                "get-access-token",
                "--resource",
                resource,
                "--query",
                "accessToken",
                "--output",
                "tsv",
            ])
            .output()
            .map_err(|error| {
                format!("run Azure CLI token lookup: {error}; run `az login` and retry")
            })?;
        if !output.status.success() {
            return Err("Azure CLI token lookup failed; run `az login` and retry".into());
        }
        let token = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        if token.is_empty() {
            Err("Azure CLI returned an empty access token; run `az login` and retry".into())
        } else {
            Ok(token)
        }
    }
}

pub struct AzureProvider<C = CommandAzureCli, H = TcpHttpTransport> {
    cli: C,
    transport: H,
    pub subscription: String,
    resource_group: String,
    graph_token: Option<String>,
    arm_token: Option<String>,
}

impl AzureProvider {
    pub fn new(subscription: String, resource_group: String) -> Self {
        Self::with(
            CommandAzureCli,
            TcpHttpTransport,
            subscription,
            resource_group,
        )
    }
}

impl<C, H> AzureProvider<C, H> {
    pub fn with(cli: C, transport: H, subscription: String, resource_group: String) -> Self {
        Self {
            cli,
            transport,
            subscription,
            resource_group,
            graph_token: None,
            arm_token: None,
        }
    }
}

impl<C: AzureCli, H: HttpTransport> AzureProvider<C, H> {
    pub fn ground(&mut self) -> Result<Account, String> {
        let account = self.cli.account()?;
        if self.subscription.is_empty() {
            self.subscription.clone_from(&account.subscription);
        }
        Ok(account)
    }

    fn request(&mut self, method: &str, url: &str, body: Option<Value>) -> Result<Value, String> {
        let graph = url.starts_with("https://graph.microsoft.com/");
        let allowed = graph || url.starts_with("https://management.azure.com/");
        if !allowed {
            return Err("refuse credential transmission to an unsupported origin".into());
        }
        let token = if graph {
            if self.graph_token.is_none() {
                self.graph_token = Some(self.cli.token("https://graph.microsoft.com")?);
            }
            self.graph_token.as_deref().unwrap_or_default()
        } else {
            if self.arm_token.is_none() {
                self.arm_token = Some(self.cli.token("https://management.azure.com")?);
            }
            self.arm_token.as_deref().unwrap_or_default()
        };
        let encoded = body.map(|value| serde_json::to_vec(&value).expect("JSON values serialize"));
        let mut headers = vec![
            ("Authorization".into(), format!("Bearer {token}")),
            ("Accept".into(), "application/json".into()),
        ];
        if encoded.is_some() {
            headers.push(("Content-Type".into(), "application/json".into()));
        }
        let response = self
            .transport
            .send(&HttpRequest {
                method: method.into(),
                url: url.into(),
                headers,
                body: encoded,
            })
            .map_err(|error| format!("send provider request: {error}"))?;
        if !(200..300).contains(&response.status) {
            let detail = sanitize_body(&response.body);
            return Err(if detail.is_empty() {
                format!("provider request failed with HTTP {}", response.status)
            } else {
                format!(
                    "provider request failed with HTTP {}: {detail}",
                    response.status
                )
            });
        }
        if response.body.is_empty() {
            return Ok(Value::Null);
        }
        serde_json::from_slice(&response.body)
            .map_err(|_| "provider returned malformed JSON".into())
    }

    fn pages(&mut self, mut url: String) -> Result<Vec<Value>, String> {
        let mut values = Vec::new();
        while !url.is_empty() {
            let page = self.request("GET", &url, None)?;
            if let Some(items) = page.get("value").and_then(Value::as_array) {
                values.extend(items.iter().cloned());
            }
            url = page
                .get("nextLink")
                .or_else(|| page.get("@odata.nextLink"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
        }
        Ok(values)
    }

    fn find_id(
        &mut self,
        collection: &str,
        field: &str,
        value: &str,
    ) -> Result<Option<String>, String> {
        let filter = percent_encode(&format!("{field} eq '{value}'"));
        let url = format!("{GRAPH}/{collection}?$select=id&$filter={filter}");
        Ok(self
            .request("GET", &url, None)?
            .get("value")
            .and_then(Value::as_array)
            .and_then(|items| items.first())
            .and_then(|item| item.get("id"))
            .and_then(Value::as_str)
            .map(str::to_owned))
    }

    fn list_refs(
        &mut self,
        path: &str,
        include_service_principals: bool,
    ) -> Result<Vec<String>, String> {
        let mut ids: BTreeSet<String> = self
            .pages(format!("{GRAPH}{path}?$select=id"))?
            .iter()
            .filter_map(|item| item.get("id").and_then(Value::as_str).map(str::to_owned))
            .collect();
        if include_service_principals {
            ids.extend(
                self.pages(format!(
                    "{GRAPH}{path}/microsoft.graph.servicePrincipal?$select=id"
                ))?
                .iter()
                .filter_map(|item| item.get("id").and_then(Value::as_str).map(str::to_owned)),
            );
        }
        Ok(ids.into_iter().collect())
    }

    fn sync_refs(&mut self, path: &str, desired: &[String]) -> Result<(), String> {
        let current: BTreeSet<_> = self.list_refs(path, false)?.into_iter().collect();
        let desired: BTreeSet<_> = desired.iter().cloned().collect();
        for id in desired.difference(&current) {
            self.request(
                "POST",
                &format!("{GRAPH}{path}/$ref"),
                Some(json!({"@odata.id": format!("{GRAPH}/directoryObjects/{id}")})),
            )?;
        }
        for id in current.difference(&desired) {
            self.request("DELETE", &format!("{GRAPH}{path}/{id}/$ref"), None)?;
        }
        Ok(())
    }

    fn list_names(&mut self, collection: &str) -> Result<Vec<String>, String> {
        Ok(self
            .pages(format!("{GRAPH}/{collection}?$select=displayName&$top=999"))?
            .iter()
            .filter_map(|item| {
                item.get("displayName")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            })
            .collect())
    }
}

impl<C: AzureCli, H: HttpTransport> Provider for AzureProvider<C, H> {
    fn ensure_zone(&mut self, zone: &str) -> Result<(), String> {
        self.require_dns_target()?;
        self.request("PUT", &format!("{ARM}/subscriptions/{}/resourceGroups/{}/providers/Microsoft.Network/dnsZones/{}?api-version={DNS_API}", pct(&self.subscription), pct(&self.resource_group), pct(zone)), Some(json!({"location":"global"}))).map(|_| ())
    }

    fn list_dns(&mut self, zone: &str) -> Result<Vec<DnsRecord>, String> {
        self.require_dns_target()?;
        let url = format!(
            "{ARM}/subscriptions/{}/resourceGroups/{}/providers/Microsoft.Network/dnsZones/{}/recordsets?api-version={DNS_API}",
            pct(&self.subscription),
            pct(&self.resource_group),
            pct(zone)
        );
        self.pages(url)?
            .iter()
            .filter_map(|item| decode_dns(zone, item))
            .collect()
    }

    fn put_dns(&mut self, record: &DnsRecord) -> Result<(), String> {
        self.require_dns_target()?;
        let body = encode_dns(record)?;
        let url = format!(
            "{ARM}/subscriptions/{}/resourceGroups/{}/providers/Microsoft.Network/dnsZones/{}/{}/{}?api-version={DNS_API}",
            pct(&self.subscription),
            pct(&self.resource_group),
            pct(&record.zone),
            pct(&record.record_type.to_uppercase()),
            pct(&record.name)
        );
        self.request("PUT", &url, Some(body)).map(|_| ())
    }

    fn delete_dns(&mut self, record: &DnsRecord) -> Result<(), String> {
        self.require_dns_target()?;
        let url = format!(
            "{ARM}/subscriptions/{}/resourceGroups/{}/providers/Microsoft.Network/dnsZones/{}/{}/{}?api-version={DNS_API}",
            pct(&self.subscription),
            pct(&self.resource_group),
            pct(&record.zone),
            pct(&record.record_type.to_uppercase()),
            pct(&record.name)
        );
        self.request("DELETE", &url, None).map(|_| ())
    }

    fn get_group(&mut self, name: &str) -> Result<Option<SecurityGroup>, String> {
        let Some(id) = self.find_id("groups", "displayName", name)? else {
            return Ok(None);
        };
        Ok(Some(SecurityGroup {
            name: name.into(),
            owners: self.list_refs(&format!("/groups/{id}/owners"), true)?,
            members: self.list_refs(&format!("/groups/{id}/members"), true)?,
        }))
    }
    fn list_group_names(&mut self) -> Result<Vec<String>, String> {
        self.list_names("groups")
    }
    fn put_group(&mut self, group: &SecurityGroup) -> Result<(), String> {
        let id = if let Some(id) = self.find_id("groups", "displayName", &group.name)? {
            id
        } else {
            self.request("POST", &format!("{GRAPH}/groups"), Some(json!({"displayName":group.name,"mailEnabled":false,"mailNickname":nickname(&group.name),"securityEnabled":true})))?.get("id").and_then(Value::as_str).ok_or("Graph group response omitted id")?.to_owned()
        };
        self.sync_refs(&format!("/groups/{id}/owners"), &group.owners)?;
        self.sync_refs(&format!("/groups/{id}/members"), &group.members)
    }
    fn delete_group(&mut self, name: &str) -> Result<(), String> {
        if let Some(id) = self.find_id("groups", "displayName", name)? {
            self.request("DELETE", &format!("{GRAPH}/groups/{id}"), None)?;
        }
        Ok(())
    }

    fn get_app(&mut self, name: &str) -> Result<Option<AppRegistration>, String> {
        let filter = percent_encode(&format!("displayName eq '{name}'"));
        let result = self.request(
            "GET",
            &format!("{GRAPH}/applications?$select=id,appId&$filter={filter}"),
            None,
        )?;
        let Some(item) = result
            .get("value")
            .and_then(Value::as_array)
            .and_then(|items| items.first())
        else {
            return Ok(None);
        };
        let id = string(item, "id");
        let app_id = string(item, "appId");
        let owners = self.list_refs(&format!("/applications/{id}/owners"), true)?;
        let exists = !self
            .request(
                "GET",
                &format!(
                    "{GRAPH}/servicePrincipals?$select=id&$filter={}",
                    percent_encode(&format!("appId eq '{app_id}'"))
                ),
                None,
            )?
            .get("value")
            .and_then(Value::as_array)
            .is_none_or(Vec::is_empty);
        Ok(Some(AppRegistration {
            name: name.into(),
            owners,
            service_principal: exists,
        }))
    }
    fn list_app_names(&mut self) -> Result<Vec<String>, String> {
        self.list_names("applications")
    }
    fn put_app(&mut self, app: &AppRegistration) -> Result<(), String> {
        let filter = percent_encode(&format!("displayName eq '{}'", app.name));
        let found = self.request(
            "GET",
            &format!("{GRAPH}/applications?$select=id,appId&$filter={filter}"),
            None,
        )?;
        let (id, app_id) = if let Some(item) = found
            .get("value")
            .and_then(Value::as_array)
            .and_then(|items| items.first())
        {
            (string(item, "id"), string(item, "appId"))
        } else {
            let created = self.request(
                "POST",
                &format!("{GRAPH}/applications"),
                Some(json!({"displayName":app.name})),
            )?;
            (string(&created, "id"), string(&created, "appId"))
        };
        self.sync_refs(&format!("/applications/{id}/owners"), &app.owners)?;
        if app.service_principal {
            let exists = !self
                .request(
                    "GET",
                    &format!(
                        "{GRAPH}/servicePrincipals?$select=id&$filter={}",
                        percent_encode(&format!("appId eq '{app_id}'"))
                    ),
                    None,
                )?
                .get("value")
                .and_then(Value::as_array)
                .is_none_or(Vec::is_empty);
            if !exists {
                self.request(
                    "POST",
                    &format!("{GRAPH}/servicePrincipals"),
                    Some(json!({"appId":app_id})),
                )?;
            }
        }
        Ok(())
    }
    fn delete_app(&mut self, name: &str) -> Result<(), String> {
        if let Some(id) = self.find_id("applications", "displayName", name)? {
            self.request("DELETE", &format!("{GRAPH}/applications/{id}"), None)?;
        }
        Ok(())
    }

    fn list_role_definitions(&mut self, subscription: &str) -> Result<Vec<RoleDefinition>, String> {
        let url = format!(
            "{ARM}/subscriptions/{}/providers/Microsoft.Authorization/roleDefinitions?api-version={ROLE_API}&$filter={}",
            pct(subscription),
            percent_encode("type eq 'CustomRole'")
        );
        Ok(self.pages(url)?.iter().map(decode_role).collect())
    }
    fn put_role_definition(
        &mut self,
        role: &RoleDefinition,
        subscription: &str,
    ) -> Result<(), String> {
        let existing = self.resolve_role(&role.name, subscription).ok();
        let id = existing
            .as_deref()
            .and_then(|value| value.rsplit('/').next())
            .map(str::to_owned)
            .unwrap_or_else(|| deterministic_uuid(&format!("role|{}", role.name)));
        let scopes: Vec<String> = if role.assignable_scopes.is_empty() {
            vec![format!("/subscriptions/{subscription}")]
        } else {
            role.assignable_scopes
                .iter()
                .map(|scope| scope.arm_id(subscription))
                .collect::<Result<_, _>>()?
        };
        let target_scope = scopes
            .first()
            .ok_or("role definition needs an assignable scope")?
            .clone();
        let body = json!({"properties":{"roleName":role.name,"description":role.description,"type":"CustomRole","assignableScopes":scopes,"permissions":[{"actions":role.actions,"notActions":role.not_actions,"dataActions":role.data_actions,"notDataActions":role.not_data_actions}]}});
        self.request("PUT", &format!("{ARM}{target_scope}/providers/Microsoft.Authorization/roleDefinitions/{id}?api-version={ROLE_API}"), Some(body)).map(|_| ())
    }
    fn delete_role_definition(&mut self, name: &str, subscription: &str) -> Result<(), String> {
        let id = self.resolve_role(name, subscription)?;
        self.request("DELETE", &format!("{ARM}{id}?api-version={ROLE_API}"), None)
            .map(|_| ())
    }
    fn resolve_principal(&mut self, name: &str, principal_type: &str) -> Result<String, String> {
        if is_directory_object_id(name) {
            return Ok(name.into());
        }
        let collection = match principal_type.to_ascii_lowercase().as_str() {
            "group" | "securitygroup" => "groups",
            "serviceprincipal" => "servicePrincipals",
            "user" => "users",
            _ => {
                return Err(
                    "unknown principalType; use group, securityGroup, servicePrincipal, or user"
                        .into(),
                );
            }
        };
        self.find_id(collection, "displayName", name)?
            .ok_or_else(|| format!("{principal_type} was not found"))
    }
    fn resolve_role(&mut self, name: &str, subscription: &str) -> Result<String, String> {
        let url = format!(
            "{ARM}/subscriptions/{}/providers/Microsoft.Authorization/roleDefinitions?api-version={ROLE_API}&$filter={}",
            pct(subscription),
            percent_encode(&format!("roleName eq '{name}'"))
        );
        self.request("GET", &url, None)?
            .get("value")
            .and_then(Value::as_array)
            .and_then(|items| items.first())
            .and_then(|item| item.get("id"))
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| "role was not found".into())
    }
    fn list_role_assignments(&mut self, scope: &str) -> Result<Vec<LiveAssignment>, String> {
        Ok(self.pages(format!("{ARM}{scope}/providers/Microsoft.Authorization/roleAssignments?api-version={ROLE_API}"))?.iter().map(|item| LiveAssignment { id: string(item, "id"), principal_id: item.pointer("/properties/principalId").and_then(Value::as_str).unwrap_or_default().into(), role_id: item.pointer("/properties/roleDefinitionId").and_then(Value::as_str).unwrap_or_default().into(), scope: item.pointer("/properties/scope").and_then(Value::as_str).unwrap_or(scope).into() }).collect())
    }
    fn put_role_assignment(
        &mut self,
        principal: &str,
        role: &str,
        scope: &str,
    ) -> Result<(), String> {
        let id = deterministic_uuid(&format!("{scope}|{principal}|{role}"));
        self.request("PUT", &format!("{ARM}{scope}/providers/Microsoft.Authorization/roleAssignments/{id}?api-version={ROLE_API}"), Some(json!({"properties":{"roleDefinitionId":role,"principalId":principal}}))).map(|_| ())
    }
    fn delete_role_assignment(&mut self, assignment: &LiveAssignment) -> Result<(), String> {
        self.request(
            "DELETE",
            &format!("{ARM}{}?api-version={ROLE_API}", assignment.id),
            None,
        )
        .map(|_| ())
    }

    fn list_resource_groups(&mut self) -> Result<Vec<ResourceGroup>, String> {
        Ok(self
            .pages(format!(
                "{ARM}/subscriptions/{}/resourcegroups?api-version={GROUP_API}",
                pct(&self.subscription)
            ))?
            .iter()
            .map(|item| ResourceGroup {
                name: string(item, "name"),
                location: string(item, "location"),
                tags: item
                    .get("tags")
                    .and_then(Value::as_object)
                    .map(|tags| {
                        tags.iter()
                            .filter_map(|(key, value)| {
                                value.as_str().map(|value| (key.clone(), value.into()))
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
            })
            .collect())
    }
    fn put_resource_group(&mut self, group: &ResourceGroup) -> Result<(), String> {
        self.request(
            "PUT",
            &format!(
                "{ARM}/subscriptions/{}/resourcegroups/{}?api-version={GROUP_API}",
                pct(&self.subscription),
                pct(&group.name)
            ),
            Some(json!({"location":group.location,"tags":group.tags})),
        )
        .map(|_| ())
    }
    fn delete_resource_group(&mut self, name: &str) -> Result<(), String> {
        self.request(
            "DELETE",
            &format!(
                "{ARM}/subscriptions/{}/resourcegroups/{}?api-version={GROUP_API}",
                pct(&self.subscription),
                pct(name)
            ),
            None,
        )
        .map(|_| ())
    }
}

impl<C, H> AzureProvider<C, H> {
    fn require_dns_target(&self) -> Result<(), String> {
        if self.subscription.is_empty() {
            return Err("Azure subscription is required; set -S or ARM_SUBSCRIPTION_ID".into());
        }
        if self.resource_group.is_empty() {
            return Err("Azure resource group is required; set -g or ARM_RESOURCE_GROUP".into());
        }
        Ok(())
    }
}

fn string(value: &Value, field: &str) -> String {
    value
        .get(field)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}
fn pct(value: &str) -> String {
    percent_encode(value)
}
fn percent_encode(value: &str) -> String {
    value
        .bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
                (byte as char).to_string()
            } else {
                format!("%{byte:02X}")
            }
        })
        .collect()
}
fn deterministic_uuid(value: &str) -> String {
    const URL_NAMESPACE: [u8; 16] = [
        0x6b, 0xa7, 0xb8, 0x11, 0x9d, 0xad, 0x11, 0xd1, 0x80, 0xb4, 0x00, 0xc0, 0x4f, 0xd4, 0x30,
        0xc8,
    ];
    let mut namespaced = URL_NAMESPACE.to_vec();
    namespaced.extend_from_slice(value.as_bytes());
    let digest = sha1(&namespaced);
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x50;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0],
        bytes[1],
        bytes[2],
        bytes[3],
        bytes[4],
        bytes[5],
        bytes[6],
        bytes[7],
        bytes[8],
        bytes[9],
        bytes[10],
        bytes[11],
        bytes[12],
        bytes[13],
        bytes[14],
        bytes[15]
    )
}
fn nickname(name: &str) -> String {
    let value: String = name.chars().filter(char::is_ascii_alphanumeric).collect();
    if value.is_empty() {
        "group".into()
    } else {
        value
    }
}
fn sanitize_body(body: &[u8]) -> String {
    if body.is_empty() {
        String::new()
    } else {
        "provider response was redacted".into()
    }
}

fn decode_role(item: &Value) -> RoleDefinition {
    let props = item.get("properties").unwrap_or(&Value::Null);
    let permission = props
        .get("permissions")
        .and_then(Value::as_array)
        .and_then(|values| values.first())
        .unwrap_or(&Value::Null);
    RoleDefinition {
        name: string(props, "roleName"),
        description: string(props, "description"),
        assignable_scopes: strings(props, "assignableScopes")
            .iter()
            .map(|scope| parse_arm_scope(scope))
            .collect(),
        actions: strings(permission, "actions"),
        not_actions: strings(permission, "notActions"),
        data_actions: strings(permission, "dataActions"),
        not_data_actions: strings(permission, "notDataActions"),
    }
}

fn parse_arm_scope(value: &str) -> Scope {
    let parts: Vec<_> = value.trim_matches('/').split('/').collect();
    let after = |name: &str| {
        parts
            .windows(2)
            .find(|pair| pair[0].eq_ignore_ascii_case(name))
            .map(|pair| pair[1])
            .unwrap_or_default()
            .to_owned()
    };
    Scope {
        management_group: after("managementGroups"),
        subscription: after("subscriptions"),
        resource_group: after("resourceGroups"),
        dns_zone: after("dnsZones"),
    }
}
fn strings(value: &Value, field: &str) -> Vec<String> {
    value
        .get(field)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}
fn decode_dns(zone: &str, item: &Value) -> Option<Result<DnsRecord, String>> {
    let name = item.get("name")?.as_str()?.to_owned();
    let record_type = item
        .get("type")?
        .as_str()?
        .rsplit('/')
        .next()?
        .to_ascii_uppercase();
    if record_type == "SOA" {
        return None;
    }
    let properties = item.get("properties")?;
    let ttl = properties.get("TTL").and_then(Value::as_i64).unwrap_or(0);
    let field = match record_type.as_str() {
        "A" => "ARecords",
        "AAAA" => "AAAARecords",
        "MX" => "MXRecords",
        "NS" => "NSRecords",
        "PTR" => "PTRRecords",
        "SRV" => "SRVRecords",
        "TXT" => "TXTRecords",
        "CNAME" => "CNAMERecord",
        _ => return None,
    };
    let values = decode_dns_values(&record_type, properties.get(field));
    Some(Ok(DnsRecord {
        zone: zone.into(),
        record_type,
        name,
        ttl,
        values,
    }))
}
fn decode_dns_values(kind: &str, value: Option<&Value>) -> Vec<String> {
    if kind == "CNAME" {
        return value
            .and_then(|v| v.get("cname"))
            .and_then(Value::as_str)
            .map(|v| vec![v.into()])
            .unwrap_or_default();
    }
    let Some(items) = value.and_then(Value::as_array) else {
        return vec![];
    };
    items
        .iter()
        .filter_map(|item| match kind {
            "A" => item
                .get("ipv4Address")
                .and_then(Value::as_str)
                .map(str::to_owned),
            "AAAA" => item
                .get("ipv6Address")
                .and_then(Value::as_str)
                .map(str::to_owned),
            "NS" => item
                .get("nsdname")
                .and_then(Value::as_str)
                .map(str::to_owned),
            "PTR" => item
                .get("ptrdname")
                .and_then(Value::as_str)
                .map(str::to_owned),
            "MX" => Some(format!(
                "{} {}",
                item.get("preference")?.as_i64()?,
                item.get("exchange")?.as_str()?
            )),
            "SRV" => Some(format!(
                "{} {} {} {}",
                item.get("priority")?.as_i64()?,
                item.get("weight")?.as_i64()?,
                item.get("port")?.as_i64()?,
                item.get("target")?.as_str()?
            )),
            "TXT" => Some(
                item.get("value")?
                    .as_array()?
                    .iter()
                    .filter_map(Value::as_str)
                    .collect::<String>(),
            ),
            _ => None,
        })
        .collect()
}
fn encode_dns(record: &DnsRecord) -> Result<Value, String> {
    let values = &record.values;
    let data = match record.record_type.to_ascii_uppercase().as_str() {
        "A" => {
            json!({"ARecords":values.iter().map(|v| json!({"ipv4Address":v})).collect::<Vec<_>>() })
        }
        "AAAA" => {
            json!({"AAAARecords":values.iter().map(|v| json!({"ipv6Address":v})).collect::<Vec<_>>() })
        }
        "CNAME" if values.len() == 1 => json!({"CNAMERecord":{"cname":values[0]}}),
        "NS" => {
            json!({"NSRecords":values.iter().map(|v| json!({"nsdname":v})).collect::<Vec<_>>() })
        }
        "PTR" => {
            json!({"PTRRecords":values.iter().map(|v| json!({"ptrdname":v})).collect::<Vec<_>>() })
        }
        "TXT" => {
            json!({"TXTRecords":values.iter().map(|v| json!({"value":[v]})).collect::<Vec<_>>() })
        }
        "MX" => {
            json!({"MXRecords":values.iter().map(|v| { let mut p=v.split_whitespace(); json!({"preference":p.next().and_then(|n|n.parse::<i64>().ok()).unwrap_or(0),"exchange":p.collect::<Vec<_>>().join(" ")}) }).collect::<Vec<_>>() })
        }
        "SRV" => {
            json!({"SRVRecords":values.iter().map(|v| { let p:Vec<_>=v.split_whitespace().collect(); json!({"priority":p.first().and_then(|n|n.parse::<i64>().ok()).unwrap_or(0),"weight":p.get(1).and_then(|n|n.parse::<i64>().ok()).unwrap_or(0),"port":p.get(2).and_then(|n|n.parse::<i64>().ok()).unwrap_or(0),"target":p.get(3..).unwrap_or_default().join(" ")}) }).collect::<Vec<_>>() })
        }
        _ => return Err("unsupported DNS record type or malformed values".into()),
    };
    let mut props = data.as_object().cloned().unwrap_or_default();
    props.insert("TTL".into(), json!(record.ttl));
    Ok(json!({"properties":props}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pman::HttpResponse;
    use std::cell::RefCell;
    use std::io;
    use std::rc::Rc;

    struct SyntheticCli;

    impl AzureCli for SyntheticCli {
        fn account(&self) -> Result<Account, String> {
            Ok(Account::default())
        }

        fn token(&self, _: &str) -> Result<String, String> {
            Ok("synthetic-token".into())
        }
    }

    #[derive(Clone, Default)]
    struct SyntheticTransport {
        urls: Rc<RefCell<Vec<String>>>,
    }

    impl HttpTransport for SyntheticTransport {
        fn send(&self, request: &HttpRequest) -> io::Result<HttpResponse> {
            self.urls.borrow_mut().push(request.url.clone());
            Ok(HttpResponse {
                status: 200,
                body: br#"{"value":[{"id":"00000000-0000-0000-0000-000000000002"}]}"#.to_vec(),
            })
        }
    }

    #[test]
    fn security_group_alias_uses_group_lookup_case_insensitively() {
        let transport = SyntheticTransport::default();
        let urls = Rc::clone(&transport.urls);
        let mut provider =
            AzureProvider::with(SyntheticCli, transport, String::new(), String::new());

        let id = provider
            .resolve_principal("synthetic-group", "SeCuRiTyGrOuP")
            .unwrap();

        assert_eq!(id, "00000000-0000-0000-0000-000000000002");
        assert_eq!(urls.borrow().len(), 1);
        assert!(urls.borrow()[0].contains("/groups?"));
    }

    #[test]
    fn literal_principal_id_skips_directory_lookup_without_a_type() {
        let transport = SyntheticTransport::default();
        let urls = Rc::clone(&transport.urls);
        let mut provider =
            AzureProvider::with(SyntheticCli, transport, String::new(), String::new());
        let principal = "00000000-0000-0000-0000-000000000003";

        assert_eq!(
            provider.resolve_principal(principal, "").unwrap(),
            principal
        );
        assert!(urls.borrow().is_empty());
    }

    #[test]
    fn origin_encoding_is_strict() {
        assert_eq!(percent_encode("a b/'"), "a%20b%2F%27");
    }
    #[test]
    fn role_ids_are_stable() {
        assert_eq!(deterministic_uuid("same"), deterministic_uuid("same"));
    }
    #[test]
    fn arm_scopes_round_trip_into_typed_fields() {
        let scope = parse_arm_scope(
            "/subscriptions/00000000-0000-0000-0000-000000000001/resourceGroups/example-resources/providers/Microsoft.Network/dnsZones/example.com",
        );
        assert_eq!(scope.resource_group, "example-resources");
        assert_eq!(scope.dns_zone, "example.com");
        assert_eq!(
            scope.arm_id("").unwrap(),
            "/subscriptions/00000000-0000-0000-0000-000000000001/resourceGroups/example-resources/providers/Microsoft.Network/dnsZones/example.com"
        );
    }
    #[test]
    fn sensitive_errors_are_redacted() {
        assert_eq!(
            sanitize_body(br#"{"access_token":"secret"}"#),
            "provider response was redacted"
        );
    }
}
