# Rango Python Binding

Python bindings for Rango memory substrate via PyO3.

## Quick Start

```python
import rango

# Open a workspace
client = rango.connect("/tmp/my-workspace")

# Get a collection
memories = client.collection("memories")

# Insert
doc_id = memories.insert_one({"content": "hello world"})

# Find
doc = memories.find_one(doc_id)
print(doc)  # {'content': 'hello world', '_id': '...', '_rev': '...'}

# Update
memories.update_one(doc_id, {"content": "updated"})

# Delete
memories.delete_one(doc_id)
```

## Development

```bash
cd crates/python
pip install maturin
maturin develop
python -m pytest tests/
```

## Build Wheel

```bash
maturin build --release
```

## Architecture

- `_core` (Rust/PyO3): Thin binding over `rango-sdk-rust` with `RedbStorage` + `FileOplog`
- `rango` (Python): DX-friendly wrapper with dict-based API
