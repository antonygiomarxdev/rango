# ADR-002: ID Generation Strategy

## Status
Accepted

## Decision
- Documentos nuevos: UUID v7 (BSON Binary subtype 0x04).
- Ingesta de payloads BSON existentes: ObjectId preservado tal cual (BSON type 0x07).

## Rationale
- UUID v7 evita clock skew y bloqueos por entropia en dispositivos embebidos.
- Preservar ObjectId mantiene compatibilidad con historiales BSON ya existentes.

## Consequences
- El SDK debe manejar ambos tipos transparentemente.
- Las comparaciones de `_id` deben respetar semantica BSON.
