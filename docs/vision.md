# Rango Vision

## Mission
Crear el mejor motor documental embebido local-first para sistemas edge y offline.

## What This Is
Rango es una base de datos documental embebida en Rust donde los documentos,
el sync incremental y la semántica de colecciones son parte del core — no una
capa sobre SQL + JSON.

## What This Is Not
- No es un reemplazo completo de MongoDB.
- No es SQLite con JSON.
- No es una solución analytics.

## Principles
1. **Local-first real**: la copia útil del dato vive localmente.
2. **Semántica documental**: documentos como primitiva natural.
3. **Sync incremental**: solo cambios, no snapshots completos.
4. **Extensibilidad**: traits estables para storage, index, sync transport.
