# Deep Example: Add AI-Generated Relationships to a Civilization Graph

## Intent

Allow AI to propose relationships between thinkers, schools, works, and historical events while keeping the graph trustworthy, explainable, and reversible.

The goal is not merely to generate more edges. The goal is to help users explore intellectual influence without mixing speculative AI output with verified historical knowledge.

## Evidence Reviewed

- Existing graph nodes support thinker, school, work, and event types.
- Current relationships are stored as directed edges.
- Existing UI does not distinguish source confidence.
- Deleting a node cascades to its outgoing and incoming edges.
- No review workflow exists for machine-generated content.
- Current search indexes node titles and descriptions but not edge evidence.

## Confirmed Facts

1. The graph already contains curated relationships.
2. AI-generated relationships would be indistinguishable from curated ones under the current model.
3. Existing edge records do not store evidence, source type, confidence, or review status.
4. The feature affects data model, search, visualization, moderation, and future migration.

## Critical Unknowns

| Unknown | Category | Impact | Disposition | Resolution |
|---|---|---:|---|---|
| Whether AI edges may become visible before human review | Known unknown | 5 | Blocker | Product decision |
| How to represent disputed or multi-directional influence | Unknown unknown candidate | 5 | Decision | Data-model prototype |
| Whether one edge can contain multiple sources | Known unknown | 4 | Decision | Schema spike |
| How users distinguish verified, community, and AI claims | Unknown known | 5 | Experiment | Contrasting visual prototypes |
| How generated edges are invalidated when prompts or models change | Unknown unknown candidate | 4 | Decision | Provenance design |
| Whether deleted nodes should preserve historical proposal records | Unknown unknown candidate | 4 | Decision | Lifecycle review |
| How search ranking should treat unreviewed edges | Known unknown | 3 | Monitor | Search experiment |

## Implicit Expectations

The product appears to value philosophical depth and historical credibility. Users are likely to interpret any visible relationship as an editorial claim, not a casual suggestion.

This expectation must be surfaced through prototypes showing at least three states:

1. Verified relationship
2. AI-proposed relationship awaiting review
3. Disputed or alternative interpretation

## Blind-Spot Candidates

### Provenance drift

A generated edge may remain after its original model, prompt, source set, or confidence policy changes.

Validation:

- Define immutable generation metadata.
- Test re-generation and invalidation flows.
- Review whether old proposals remain auditable.

### False authority from visual treatment

Even a low-confidence dashed line may still be perceived as factual if placed in the same graph.

Validation:

- Conduct a small usability test.
- Compare separate-layer, toggle, and review-inbox designs.
- Require explicit source inspection before promotion.

### Graph pollution at scale

Bulk generation can create dense, low-value edges that reduce readability and search quality.

Validation:

- Generate proposals against a representative dataset.
- Measure edge density and navigation quality.
- Add per-node and per-run limits.

### Historical ambiguity

Influence may be indirect, disputed, parallel, or mediated through translations and later interpreters.

Validation:

- Expand edge taxonomy.
- Allow evidence notes and competing interpretations.
- Avoid a single generic `influenced` relation.

## Decisions Required

1. Are AI proposals invisible until reviewed, or visible in a separate exploration layer?
2. Who may approve a proposal?
3. Is approval global, workspace-specific, or user-specific?
4. Can one relationship contain conflicting sources?
5. What confidence or evidence threshold is required?
6. How are rejected proposals retained for audit and future model evaluation?

## Experiments Required

### Data-model spike

Compare:

- Extending the existing edge table
- Separate proposal table promoted into canonical edges
- Event-sourced relationship claims

Recommendation:

Use a separate proposal model. Promotion should create or update a canonical relationship while preserving immutable provenance.

### UX prototypes

Build three prototypes:

1. Inline dashed AI edges
2. Optional AI exploration layer
3. Review inbox with side-by-side evidence

Recommendation:

Prefer the optional exploration layer plus review inbox. Do not mix unreviewed proposals into the default canonical graph.

## Recommended Data Boundary

### Canonical relationship

- Stable relation identifier
- Relation type
- Source and target
- Editorial status
- Human-readable explanation
- Verified evidence references
- Created and updated timestamps

### AI proposal

- Proposal identifier
- Proposed relation type
- Source and target
- Model and prompt version
- Evidence candidates
- Confidence
- Generation run
- Review status
- Reviewer and review time
- Rejection reason
- Promotion target
- Invalidation metadata

## Rollback Requirements

- Disable AI proposal generation independently from graph browsing.
- Hide the AI layer without deleting proposals.
- Revert promoted edges created by a generation run.
- Preserve review history and evidence after rollback.
- Rebuild search indexes without AI proposal content.

## Verification Plan

### Automated

- Proposal lifecycle tests
- Promotion and rejection tests
- Permission tests
- Cascade and deletion tests
- Search-index isolation tests
- Migration rollback tests
- Duplicate and conflict tests

### Manual

- Distinguish verified and AI content at a glance
- Inspect evidence before approval
- Compare competing interpretations
- Disable the AI layer
- Recover from partial generation failure
- Review the graph at high edge density

## Final Implementation Boundary

Implement:

- Separate AI proposal storage
- Provenance and review status
- Optional AI exploration layer
- Review workflow
- Promotion and rollback
- Search isolation
- Audit history

Do not implement yet:

- Automatic publication to the canonical graph
- Bulk generation across the full dataset
- User reputation or community voting
- Autonomous conflict resolution
