"""Rango Python SDK — Pythonic wrapper over the Rust core."""

from typing import Mapping

from rango._core import RangoClient as _RangoClient

__version__ = "0.1.0"
__all__ = ["connect", "RangoClient"]

# Base document type — schemaless but structured
type Document = Mapping[str, object]


class Collection:
    """Pythonic collection interface."""

    def __init__(self, client: "RangoClient", name: str):
        self._client = client
        self._name = name

    def insert_one(self, doc: Document) -> str:
        """Insert a document and return its ID."""
        return self._client._core.insert_one(self._name, doc)

    def find_one(self, id: str) -> Document | None:
        """Find a document by ID."""
        return self._client._core.find_one(self._name, id)

    def find_many(self) -> list[Document]:
        """Find all documents in the collection."""
        return self._client._core.find_many(self._name)

    def update_one(self, id: str, update: Document) -> bool:
        """Update a document by ID."""
        return self._client._core.update_one(self._name, id, update)

    def delete_one(self, id: str) -> bool:
        """Delete a document by ID."""
        return self._client._core.delete_one(self._name, id)


class RangoClient:
    """High-level Rango client with Pythonic API."""

    def __init__(self, core: _RangoClient):
        self._core = core

    def collection(self, name: str) -> Collection:
        """Get a collection by name."""
        return Collection(self, name)

    @classmethod
    def connect(cls, path: str, node_id: str = "python-node") -> "RangoClient":
        """Open a Rango workspace at the given path."""
        core = _RangoClient(path, node_id)
        return cls(core)


def connect(path: str, node_id: str = "python-node") -> RangoClient:
    """Convenience function to open a Rango workspace."""
    return RangoClient.connect(path, node_id)
