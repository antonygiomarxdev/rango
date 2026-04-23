# ADR-003: Sync Protocol Wire Format

## Status
Accepted

## Decision
- Protocolo: HTTP/1.1 con payloads JSON.
- Header: `X-Rango-Protocol-Version: 1` en todo request.

## Rationale
- Más simple y debuggeable que gRPC para redes inestables en edge.
- JSON es universalmente parseable.
- Header de versión previene incompatibilidades futuras.

## Consequences
- Mayor tamaño de payload que binario (tradeoff aceptable para MVP).
- Fácil de implementar en cualquier lenguaje cliente.
