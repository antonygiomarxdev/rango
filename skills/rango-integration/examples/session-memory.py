# Session Memory Example — Python
# Demonstrates persisting user sessions with governance metadata

import rango
import uuid
from datetime import datetime, timedelta

def main():
    # Open workspace
    client = rango.RangoClient("./session-memory-example.rango", node_id="session-example")
    
    # Insert session with governance metadata
    session_id = str(uuid.uuid7())
    session_doc = {
        "user_id": "alice",
        "session_token": session_id,
        "created_at": datetime.utcnow().isoformat(),
        "tenant_id": "org-123",           # Governance: tenant scope
        "lineage": f"session:{session_id}", # Governance: provenance
        "trust_score": 1.0,               # Governance: verified internal
        "verified": True,
        "expires_at": (datetime.utcnow() + timedelta(hours=1)).isoformat(),
    }
    
    doc_id = client.insert_one("sessions", session_doc)
    print(f"Session created: {doc_id}")
    
    # Retrieve session
    doc = client.find_one("sessions", doc_id)
    if doc:
        print(f"Found session: {doc}")

if __name__ == "__main__":
    main()
