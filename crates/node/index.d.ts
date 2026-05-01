export class RangoClient {
  constructor(path: string, nodeId?: string)
  insertOne(collectionName: string, jsonDoc: string): string
  findOne(collectionName: string, id: string): string | null
  findMany(collectionName: string): string[]
  updateOne(collectionName: string, id: string, jsonUpdate: string): boolean
  deleteOne(collectionName: string, id: string): boolean
}

export class Collection {
  insertOne(doc: Record<string, unknown>): string
  findOne(id: string): Record<string, unknown> | null
  findMany(): Record<string, unknown>[]
  updateOne(id: string, update: Record<string, unknown>): boolean
  deleteOne(id: string): boolean
}

export class Rango {
  constructor(path: string, nodeId?: string)
  collection(name: string): Collection
}

export function connect(path: string, nodeId?: string): Rango
