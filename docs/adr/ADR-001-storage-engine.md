# ADR-001: Storage Engine Decision

## Status
Accepted - 2026-04-24

## Context
Rango necesita persistencia embebida local-first con durabilidad real para estado operativo y replay.
La arquitectura ya expone `StorageEngine` como contrato intercambiable y requiere un backend default de produccion para v0.1.0.

## Decision
- Backend default de v0.1.0: **redb**.
- `StorageEngine` permanece como contrato estable para permitir swap de backend futuro sin romper `core/sdk/server`.
- `MemoryStorage` queda restringido a test/dev.

## Rationale
- `redb` ofrece integracion embebida simple y robusta para una primera entrega de produccion local-first.
- Permite cerrar el gap actual de durabilidad en disco sin acoplar el resto del motor a un backend unico.
- Mantiene disciplina de capas: el backend es detalle de infraestructura, no semantica de producto.

## Consequences
- CLI y rutas operativas deben abrir `RedbStorage` por defecto.
- Las pruebas de recovery/persistencia deben ejecutarse sobre `redb`.
- Se preserva la opcion de agregar backend alternativo despues, sin migracion de API publica.

## Follow-ups
- Definir politicas de compaction/maintenance para el backend persistente.
- Agregar benchmark comparativo de backends cuando exista candidato real de reemplazo.
