# Rango Integration Skill

Help developers integrate Rango as a durable memory and state substrate into their applications.

## When to Use

This skill activates when:
- User explicitly says "Add Rango", "Integrate Rango", "Use Rango for memory"
- No Rango dependency found + user mentions persistence, state, memory, durable storage
- Rango dependency found + user asks "how do I persist X", "store this in Rango"
- Rango dependency found + user asks "review my Rango code", "audit my integration"
- User asks architecture questions: "should I persist this", "what should be durable"

## Modes

This skill operates in 4 modes based on context:

### Mode 1: Setup Guide
**Trigger:** Rango not installed in the project
**Action:** Detect stack, show installation, generate initialization code

### Mode 2: Pattern Generator
**Trigger:** User wants to persist specific state
**Action:** Map state to Rango primitives, generate code with governance metadata

### Mode 3: Architecture Advisor
**Trigger:** User asks where Rango fits
**Action:** Advise what to persist vs. derive, collection structure, metadata requirements

### Mode 4: Audit Reviewer
**Trigger:** Rango already used + user wants review
**Action:** Check integration against best practices checklist

## Mode Detection

1. Check if project has Rango dependency:
   - Rust: Check `Cargo.toml` for `rango-sdk` or `rango`
   - Python: Check `pyproject.toml`/`requirements.txt` for `rango`
   - Node.js: Check `package.json` for `@rango/core`

2. If no Rango dependency → **Setup Mode**

3. If Rango dependency exists:
   - User asks "how do I..." / "persist..." / "store..." → **Pattern Mode**
   - User asks "should I..." / "where does..." / "architecture..." → **Architecture Mode**
   - User asks "review..." / "audit..." / "check my..." → **Audit Mode**
   - Otherwise → ask user what they need

## Setup Mode

### Detect Stack

Read project files to determine language:
- `Cargo.toml` → Rust
- `pyproject.toml` or `requirements.txt` → Python
- `package.json` → Node.js/TypeScript

### Installation

**Rust:**
```bash
cargo add rango-sdk rango-types rango-storage
```

**Python:**
```bash
pip install rango
# or
poetry add rango
```

**Node.js/TypeScript:**
```bash
npm install @rango/core
# or
yarn add @rango/core
```

### Initialize

See `references/setup-{language}.md` for language-specific initialization code.

## Pattern Mode

See `references/patterns.md` for common persistence patterns.

## Architecture Mode

See `references/architecture.md` for design guidelines.

## Audit Mode

See `references/audit-checklist.md` for review criteria.
