// Session Memory Example — TypeScript
// Demonstrates persisting user sessions with governance metadata

import { RangoClient } from '@rango/core';
import { v7 as uuidv7 } from 'uuid';

function main() {
    // Open workspace
    const client = new RangoClient('./session-memory-example.rango', 'session-example');
    
    // Insert session with governance metadata
    const sessionId = uuidv7();
    const sessionDoc = JSON.stringify({
        user_id: 'alice',
        session_token: sessionId,
        created_at: new Date().toISOString(),
        tenant_id: 'org-123',              // Governance: tenant scope
        lineage: `session:${sessionId}`,   // Governance: provenance
        trust_score: 1.0,                 // Governance: verified internal
        verified: true,
        expires_at: new Date(Date.now() + 3600000).toISOString(),
    });
    
    const docId = client.insertOne('sessions', sessionDoc);
    console.log(`Session created: ${docId}`);
    
    // Retrieve session
    const doc = client.findOne('sessions', docId);
    if (doc) {
        console.log(`Found session: ${doc}`);
    }
}

main();
