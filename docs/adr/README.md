# Architecture Decision Records (ADRs)

This directory contains architecture decision records for QuorumCredit.

Each ADR documents a major design choice, the context behind it, and the consequences of the decision. ADRs provide a record of why specific technical decisions were made and help guide future development.

## Purpose

Architecture Decision Records (ADRs) serve to:

1. **Document decisions** - Capture important architectural choices and their rationale
2. **Preserve context** - Explain the problem, options considered, and why one was chosen
3. **Guide the team** - Help new team members understand design philosophy
4. **Facilitate discussion** - Create a shared record for architectural decisions
5. **Enable consistency** - Ensure decisions are made consciously and consistently

## ADR Process

### When to Create an ADR

Create an ADR for:
- Major architectural decisions affecting multiple components
- Significant design choices with long-term implications
- Technology selections (frameworks, languages, platforms)
- Protocol or API changes
- Operational procedures or policies

### Creating a New ADR

1. **Propose the ADR**
   - Create issue with label `adr` describing the decision
   - Invite relevant team members for discussion
   - Let discussion happen for 1-2 days minimum

2. **Write the ADR**
   - Copy `0000-template.md` to `NNNN-brief-title.md`
   - Use the next sequential number (e.g., 0006)
   - Fill out all sections completely
   - Include clear rationale and consequences

3. **Get Approval**
   - Create pull request with the ADR
   - Minimum 2 approvals from senior engineers
   - Tech lead approval for major decisions

4. **Archive Decision**
   - Merge ADR to `main` branch
   - Update this README with new ADR
   - Reference ADR number in related code/docs

### ADR Template Structure

```markdown
# ADR NNNN: Brief Title

**Date:** YYYY-MM-DD  
**Status:** Accepted | Proposed | Superseded | Deprecated  
**Author(s):** Name(s)  
**Affected Components:** List of affected systems  

## Context

[Describe the issue or decision that needs to be made]

## Decision

[State the decision clearly]

## Rationale

[Explain why this decision was made]
- Pro: benefit 1
- Pro: benefit 2
- Con: tradeoff 1
- Considered alternative: description

## Consequences

[Describe the results of the decision]
- Positive: consequence 1
- Negative: consequence 2
- Requires: related action/decision

## Alternatives Considered

### Alternative 1: [Name]
[Brief description and why rejected]

### Alternative 2: [Name]
[Brief description and why rejected]

## References

- [Related document 1]
- [Issue or PR link]
```

## Current ADRs

### Accepted (Active)

- **0001-use-soroban.md** — Use Soroban for smart contract execution.
- **0002-use-stroops-as-unit.md** — Represent all monetary values in stroops.
- **0003-fba-inspired-trust-model.md** — Base eligibility on an FBA-inspired trust model.
- **0004-yield-and-slash-model.md** — Use a 2% yield and 50% slash economic model.
- **0005-multisig-admin-and-governance.md** — Require multisig-style admin and governance for critical operations.

### Proposed (Pending)

*None at this time*

### Superseded (Historical)

*None at this time*

### Deprecated (No Longer Used)

*None at this time*

## ADR Index by Category

### Architecture & Platform

- **ADR 0001** - Soroban smart contract platform
- **ADR 0005** - Multisig admin and governance

### Design & Economics

- **ADR 0002** - Stroops as native monetary unit
- **ADR 0003** - FBA-inspired trust model
- **ADR 0004** - Yield and slash economic model

### Operational (Future)

*To be added as needed*

## Querying ADRs

### Find ADRs by Status
```bash
grep -l "^Status: Accepted" *.md
```

### Find ADRs by Component
```bash
grep -l "Affected Components:.*governance" *.md
```

### Find ADRs by Date Range
```bash
grep "^Date:" *.md | grep "202[56]"
```

## Learning Resources

### For New Team Members

1. Start with **ADR 0001** - Platform choice
2. Read **ADR 0002** - Core convention
3. Review **ADR 0003-0005** - Design principles

### For Implementers

- Check affected ADRs before architectural changes
- Reference ADR numbers in commit messages
- Link to ADRs in documentation

### For Decision Makers

- Review existing ADRs before making related decisions
- Consider creating ADR for cross-cutting concerns
- Use ADRs to facilitate team alignment

## Questions?

- **ADR Process:** See this README
- **ADR Template:** See `0000-template.md`
- **Existing ADRs:** Browse `.md` files in this directory
- **Discussions:** Use issue label `adr` in repository
