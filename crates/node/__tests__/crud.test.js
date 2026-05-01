const { RangoClient } = require('./index')

describe('RangoClient', () => {
  const tmpDir = require('os').tmpdir() + '/rango-node-test-' + Date.now()
  let client

  beforeEach(() => {
    client = new RangoClient(tmpDir)
  })

  test('insertOne returns string id', () => {
    const id = client.insertOne('memories', JSON.stringify({ content: 'hello' }))
    expect(typeof id).toBe('string')
    expect(id.length).toBeGreaterThan(0)
  })

  test('findOne returns document', () => {
    const id = client.insertOne('memories', JSON.stringify({ content: 'hello' }))
    const doc = client.findOne('memories', id)
    expect(doc).not.toBeNull()
    const parsed = JSON.parse(doc)
    expect(parsed.content).toBe('hello')
    expect(parsed._id).toBe(id)
  })

  test('findOne returns null for missing', () => {
    const doc = client.findOne('memories', 'non-existent-id')
    expect(doc).toBeNull()
  })

  test('findMany returns array', () => {
    client.insertOne('memories', JSON.stringify({ content: 'a' }))
    client.insertOne('memories', JSON.stringify({ content: 'b' }))
    const docs = client.findMany('memories')
    expect(Array.isArray(docs)).toBe(true)
    expect(docs.length).toBe(2)
  })

  test('updateOne returns boolean', () => {
    const id = client.insertOne('memories', JSON.stringify({ content: 'old' }))
    const updated = client.updateOne('memories', id, JSON.stringify({ content: 'new' }))
    expect(updated).toBe(true)
    const doc = JSON.parse(client.findOne('memories', id))
    expect(doc.content).toBe('new')
  })

  test('deleteOne returns boolean', () => {
    const id = client.insertOne('memories', JSON.stringify({ content: 'delete me' }))
    const deleted = client.deleteOne('memories', id)
    expect(deleted).toBe(true)
    expect(client.findOne('memories', id)).toBeNull()
  })
})
