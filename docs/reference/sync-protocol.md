# Sync Protocol v1

## Endpoints
- `POST /push` — enviar mutaciones locales al primario
- `POST /pull` — recibir mutaciones desde un checkpoint

## Headers
- `X-Rango-Protocol-Version: 1`
- `Authorization: Bearer <node_token>`

## Push Request
```json
{
  "node_id": "gateway-managua-1",
  "tenant_id": "farm-42",
  "mutations": [
    {
      "op": "update",
      "collection": "animals",
      "doc_id": "cow_123",
      "patch": { "$set": { "temp": 39.1 } },
      "seq": 42,
      "timestamp": "2026-04-22T20:30:00Z"
    }
  ],
  "last_checkpoint": 128
}
```

## Pull Response
```json
{
  "changes": [...],
  "new_checkpoint": 156,
  "acks": [42, 43]
}
```
