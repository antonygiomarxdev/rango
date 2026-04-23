# ADR-002: ID Generation Strategy

## Status
Accepted

## Decision
- Documentos nuevos: UUID v7 (BSON Binary subtype 0x04).
- Import desde MongoDB: ObjectId preservado tal cual (BSON type 0x07).

## Rationale
- UUID v7 evita clock skew y bloqueos por entropía en dispositivos embebidos.
- Preservar ObjectId mantiene compatibilidad con datos MongoDB existentes.

## Consequences
- El SDK debe manejar ambos tipos transparentemente.
- Las comparaciones de `_id` deben respetar semántica BSON.
