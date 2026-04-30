"""Integration tests for the Rango Python binding."""

import tempfile
import pytest

import rango


@pytest.fixture
def client():
    """Create a temporary Rango client for testing."""
    with tempfile.TemporaryDirectory() as tmpdir:
        yield rango.connect(tmpdir)


@pytest.fixture
def memories(client):
    """Get the 'memories' collection."""
    return client.collection("memories")


class TestInsert:
    def test_insert_one_returns_id(self, memories):
        doc = {"content": "hello world"}
        doc_id = memories.insert_one(doc)
        assert isinstance(doc_id, str)
        assert len(doc_id) > 0

    def test_insert_one_with_nested_doc(self, memories):
        doc = {"user": {"name": "Alice"}, "tags": ["a", "b"]}
        doc_id = memories.insert_one(doc)
        assert isinstance(doc_id, str)


class TestFind:
    def test_find_one_existing(self, memories):
        doc = {"content": "find me"}
        doc_id = memories.insert_one(doc)
        found = memories.find_one(doc_id)
        assert found is not None
        assert found["content"] == "find me"
        assert "_id" in found
        assert "_rev" in found

    def test_find_one_missing(self, memories):
        found = memories.find_one("non-existent-id")
        assert found is None

    def test_find_many_empty(self, memories):
        docs = memories.find_many()
        assert docs == []

    def test_find_many_multiple(self, memories):
        ids = []
        for i in range(3):
            doc_id = memories.insert_one({"idx": i})
            ids.append(doc_id)

        docs = memories.find_many()
        assert len(docs) == 3
        assert all("_id" in d for d in docs)
        assert all("_rev" in d for d in docs)


class TestUpdate:
    def test_update_one_existing(self, memories):
        doc_id = memories.insert_one({"content": "old"})
        updated = memories.update_one(doc_id, {"content": "new"})
        assert updated is True

        found = memories.find_one(doc_id)
        assert found["content"] == "new"

    def test_update_one_missing(self, memories):
        updated = memories.update_one("non-existent", {"content": "new"})
        assert updated is False


class TestDelete:
    def test_delete_one_existing(self, memories):
        doc_id = memories.insert_one({"content": "delete me"})
        deleted = memories.delete_one(doc_id)
        assert deleted is True
        assert memories.find_one(doc_id) is None

    def test_delete_one_missing(self, memories):
        deleted = memories.delete_one("non-existent")
        assert deleted is False


class TestCollections:
    def test_multiple_collections_isolated(self, client):
        col_a = client.collection("col_a")
        col_b = client.collection("col_b")

        id_a = col_a.insert_one({"data": "a"})
        id_b = col_b.insert_one({"data": "b"})

        assert col_a.find_one(id_a) is not None
        assert col_a.find_one(id_b) is None
        assert col_b.find_one(id_b) is not None
        assert col_b.find_one(id_a) is None
