# Rango v0.0 -> v0.1 Migration Guide

## Audience

This guide is for operators managing Rango memory workspaces created with pre-v0.1 builds. If you are initializing a new workspace with v0.1 or later, you do not need to follow this guide.

## Why this migration matters

Rango v0.1 enforces a canonical envelope metadata contract end-to-end across all operations. All records must carry governance metadata fields including:

- `tenant_id` — workspace tenant identifier
- `namespace` — logical namespace for the record
- `lineage` — data provenance and audit trail
- `trust_score` — confidence score (0.0 – 1.0)
- `verified` — optional boolean flag for record verification
- `expires_at` — optional expiration timestamp
- `_rev` — revision/sequence identifier
- `_updated_at` — last-modified timestamp
- `_source_node` — originating node identifier

Legacy v0.0 records lacking these canonical fields are rejected by the runtime control-plane and cannot be read, updated, or synced. **Upgrading without migrating your data will render your workspace inaccessible.**

## Diagnose

Before migrating, verify your workspace compatibility:

```bash
rango doctor <workspace-path>
```

**Exit code 0** means your workspace is compatible with v0.1 and migration is not required.

**Non-zero exit code** means your workspace contains records with an incompatible v0.0 shape. The doctor output will list the specific records and missing fields. Proceed to the upgrade workflow below.

## Upgrade workflow

### Step 1: Stop all writers

Stop all applications and processes writing to the workspace. This prevents concurrent modifications during the migration.

```bash
# Example: stop your application
systemctl stop my-app
```

### Step 2: Snapshot the workspace

Create a backup copy of your workspace directory in case a rollback becomes necessary:

```bash
# On Linux/macOS
cp -r /path/to/workspace /path/to/workspace.backup-v0.0

# On Windows (PowerShell)
Copy-Item -Path "C:\path\to\workspace" -Destination "C:\path\to\workspace.backup-v0.0" -Recurse
```

**Keep this snapshot until you have successfully validated v0.1 in production.**

### Step 3: Export data from legacy workspace

Export all collections from the v0.0 workspace:

```bash
rango export --path <workspace> --collection <collection-name> --output <tmp>/<collection-name>.jsonl
```

Repeat for each collection in your workspace. If you have multiple collections, automate this:

```bash
# Example shell script (Linux/macOS)
for collection in documents records data items; do
  rango export --path /path/to/workspace --collection "$collection" --output /tmp/"$collection".jsonl
done
```

### Step 4: Initialize a fresh v0.1 workspace

Create a new workspace with v0.1:

```bash
rango init /path/to/workspace-v0.1
```

If your original workspace was encrypted, initialize with the same passphrase:

```bash
rango init /path/to/workspace-v0.1 --passphrase <passphrase>
```

### Step 5: Re-import data with canonical envelope defaults

Import the exported data back into the new workspace. The import path applies canonical envelope defaults to all records:

```bash
rango import --path /path/to/workspace-v0.1 --collection <collection-name> /tmp/<collection-name>.jsonl
```

Repeat for each collection. The import process will:

1. Parse each record from the JSONL file.
2. Assign canonical envelope metadata (with defaults if not present).
3. Store the record in v0.1 format.

### Step 6: Verify the upgraded workspace

Run doctor on the new workspace to confirm all records are compatible:

```bash
rango doctor /path/to/workspace-v0.1
```

You should see exit code 0 and the message "Doctor check complete."

### Step 7: Switch your application

Update your application configuration to point to the new v0.1 workspace:

```bash
# Example: update environment variable
export RANGO_WORKSPACE=/path/to/workspace-v0.1
# Restart your application
systemctl start my-app
```

Verify that your application can read, write, and sync data from the new workspace.

## Rollback

If you encounter issues with the v0.1 workspace:

1. Stop your application.
2. Switch back to the original workspace:
   ```bash
   export RANGO_WORKSPACE=/path/to/workspace.backup-v0.0
   ```
3. Restart your application.

You can then diagnose the issue and retry the migration. Keep the v0.0 backup until v0.1 is stable in production.

## What v0.1 doctor checks

`rango doctor` performs the following checks:

- **Storage Check** — verifies workspace directory exists.
- **Config Check** — reads and validates `rango.json` configuration.
- **Engine Check** — attempts to open the storage engine and oplog.
- **Operations Check** — verifies insert, find, update, and delete operations work.
- **Metadata Check** — confirms canonical envelope fields are present on new records.
- **Upgrade Check** — samples up to 20 records from existing collections and validates all canonical envelope fields. Missing fields are reported as incompatibilities.
- **Metrics Snapshot** — shows operation counters.
- **Sync Infrastructure Check** — validates oplog, sync queue, and checkpoint files.

Doctor exits with code 0 only if all checks pass. Any incompatibility or error causes a non-zero exit.

## Known limitations

- `rango doctor` currently samples up to 20 records per collection. A full workspace sweep is on the roadmap.
- Encrypted workspaces require the `--passphrase` flag to be passed to all commands.
- Doctor does not repair or migrate data automatically; it is a diagnostic tool only.

## Support

For migration questions or issues:

1. Review this guide and run `rango doctor` to identify specific incompatibilities.
2. Check [CONTRIBUTING.md](../../CONTRIBUTING.md) for security reporting and community support channels.
3. Refer to [docs/operations/security.md](security.md) for vulnerability reports.

## Additional resources

- [Rango Architecture Overview](../architecture/overview.md)
- [Sync Protocol Specification](../reference/sync-protocol.md)
- [Query Language Reference](../reference/query-language.md)
