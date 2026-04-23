# Migration Guide: From MongoDB to Rango

This guide describes how to migrate data from MongoDB to Rango and covers compatibility considerations.

## Overview

Rango provides a documental API surface compatible with MongoDB CRUD operations. Migration is straightforward for workloads that use:

- Basic CRUD (insert, find, update, delete)
- Simple queries with equality, range, and logical operators
- Single-field indexes

## Export from MongoDB

### Using `mongoexport` (JSON Lines)

The recommended format for migration is **JSON Lines** (one JSON document per line):

```bash
mongoexport \
  --db=myapp \
  --collection=users \
  --out=users.json \
  --jsonFormat=canonical
```

The `--jsonFormat=canonical` flag ensures MongoDB Extended JSON is used, which preserves BSON types like `ObjectId` and `DateTime`.

### Example exported document

```json
{"_id":{"$oid":"507f1f77bcf86cd799439011"},"name":"Alice","createdAt":{"$date":"2024-01-15T10:30:00Z"}}
```

## Import to Rango

### Using the CLI

```bash
rango import \
  --collection users \
  users.json
```

### Preserved fields

During import, Rango preserves:

- **`_id`**: If the original `_id` is an `ObjectId`, it is stored as a BSON ObjectId (not converted to UUID).
- **Document structure**: All fields are preserved as-is.
- **BSON types**: Extended JSON types (`$date`, `$numberInt`, `$numberLong`, `$numberDouble`) are converted back to their native BSON equivalents.

### `_id` strategy

| Original `_id` type | Behavior in Rango |
|---------------------|-------------------|
| `ObjectId` | Preserved as BSON ObjectId |
| `String` | Preserved as string |
| `UUID` | Preserved as BSON Binary (UUID subtype) |
| Missing | Auto-generated UUID v7 |

## Export from Rango

```bash
rango export \
  --collection users \
  --output users-export.json
```

Exported documents are written as standard JSON (not Extended JSON). ObjectIds are represented as strings for maximum compatibility.

## Compatibility Limitations

Rango v1 does **not** support the following MongoDB features:

- Aggregation pipeline
- Multi-document transactions
- Full-text search
- Geospatial queries
- Schema validation
- Change streams
- GridFS

If your workload depends on any of these, evaluate whether Rango is the right fit or if the missing features can be implemented at the application layer.

## Type Mapping

| BSON Type | mongoexport JSON | Rango Import | Rango Export |
|-----------|------------------|--------------|--------------|
| ObjectId | `{"$oid":"..."}` | BSON ObjectId | String |
| DateTime | `{"$date":"..."}` | BSON DateTime | ISO-8601 String |
| Int32 | `{"$numberInt":"..."}` | BSON Int32 | JSON Number |
| Int64 | `{"$numberLong":"..."}` | BSON Int64 | JSON Number |
| Double | `{"$numberDouble":"..."}` | BSON Double | JSON Number |
| String | `"..."` | BSON String | JSON String |
| Boolean | `true/false` | BSON Boolean | JSON Boolean |
| Array | `[...]` | BSON Array | JSON Array |
| Document | `{...}` | BSON Document | JSON Object |
| Binary | `{"$binary":{"base64":"..."}}` | BSON Binary | `<Binary ...>` |
| UUID | `{"$binary":{"base64":"...","subType":"04"}}` | BSON Binary (UUID) | UUID String |

## Batch Size and Memory

Import and export operations use **streaming I/O**:

- Documents are read/written one at a time
- Memory usage is proportional to a single document, not the entire collection
- Suitable for importing/exporting collections of any size

## Error Handling

During import:

- Invalid JSON lines are skipped and logged as errors
- Duplicate `_id` errors are logged but do not abort the batch
- The final summary reports total imported and error counts

Example output:

```
Importing into collection 'users' from users.json...
  Imported 100 documents...
  Imported 200 documents...
  Error at line 205: JSON parse error: expected `,` or `}` at column 23
  Done: 499 imported, 1 errors
Import complete: 499 documents imported, 1 errors
```

## Next Steps

After migration:

1. Verify document counts: `rango inspect`
2. Create secondary indexes for query fields
3. Set up sync to a Rango server if using distributed mode
4. Update application code to use the Rango SDK
