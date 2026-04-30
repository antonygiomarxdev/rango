# Rango Node.js Binding

TypeScript/Node.js bindings for Rango memory substrate via napi-rs.

## Quick Start

```typescript
import { connect } from './rango'

const db = connect('/tmp/workspace')
const memories = db.collection('memories')

const id = memories.insertOne({ content: 'hello world' })
const doc = memories.findOne(id)
console.log(doc) // { content: 'hello world', _id: '...', _rev: ... }

memories.updateOne(id, { content: 'updated' })
memories.deleteOne(id)
```

## Development

```bash
# Build native module
npm run build

# Run tests
npm test
```

## Architecture

- **Rust core** (`src/lib.rs`): napi-rs binding over `rango-sdk`
- **TypeScript wrapper** (`rango.ts`): DX-friendly API with JSON serialization
- **Auto-$set**: Updates without MongoDB operators are wrapped in `$set` automatically
