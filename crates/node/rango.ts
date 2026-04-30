import { RangoClient } from './index'

export class Collection {
  constructor(private client: RangoClient, private name: string) {}

  insertOne(doc: Record<string, unknown>): string {
    return this.client.insertOne(this.name, JSON.stringify(doc))
  }

  findOne(id: string): Record<string, unknown> | null {
    const result = this.client.findOne(this.name, id)
    return result ? JSON.parse(result) : null
  }

  findMany(): Record<string, unknown>[] {
    const results = this.client.findMany(this.name)
    return results.map(r => JSON.parse(r))
  }

  updateOne(id: string, update: Record<string, unknown>): boolean {
    return this.client.updateOne(this.name, id, JSON.stringify(update))
  }

  deleteOne(id: string): boolean {
    return this.client.deleteOne(this.name, id)
  }
}

export class Rango {
  private client: RangoClient

  constructor(path: string, nodeId?: string) {
    this.client = new RangoClient(path, nodeId)
  }

  collection(name: string): Collection {
    return new Collection(this.client, name)
  }
}

export function connect(path: string, nodeId?: string): Rango {
  return new Rango(path, nodeId)
}
