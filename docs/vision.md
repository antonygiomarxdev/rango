# Rango Vision

## Mission
Construir la capa de memoria durable y gobernable para sistemas de IA con estado.

## What This Is
Rango es un substrate local-first de memoria documental para continuidad operativa: estado actual, historial episodico y memoria derivada, con durabilidad, replay y sync incremental.

## What This Is Not
- No es un producto de workflows.
- No es una capa de negocio de agentes.
- No es un almacenamiento generico sin semantica de memoria.

## Principles
1. **Memory-first**: el modelo sigue como un sistema stateful recuerda y actua.
2. **State vs History**: estado operativo e historial inmutable son capas distintas.
3. **Durability-first**: oplog, checkpoints y replay antes de capas de inteligencia.
4. **Governed memory**: write/read/promotion bajo politicas explicitas.
5. **Layer discipline**: retrieval avanzado como proyeccion externa, no verdad canonica.
