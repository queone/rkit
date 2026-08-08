# Attune synthetic example

This directory is a fictional, provider-safe attune project. It demonstrates
all supported resource kinds without containing a usable tenant, subscription,
principal, credential, or production domain.

Validate the files offline from this directory:

```sh
attune validate
```

`validate` reads local configuration and specifications only. In contrast,
`attune plan` authenticates to Azure and reads live provider state, while
`attune apply` can create, update, and delete provider resources according to
the reviewed plan and enabled prune policies.

Before any live command, deliberately replace and review every placeholder
class: subscription ID, resource-group name and location, DNS zone and record
data, security-group name and relationships, app-registration name and owners,
role name and permissions, assignment principal and scope, tags, and every
prune setting. Do not run `plan` or `apply` merely by copying this directory.

The zero UUID, `example.com`, TEST-NET address, and `example-attune-*` names are
documentation placeholders. They do not identify deployable infrastructure.
