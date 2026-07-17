---
name: unknowns-discovery
description: Discover, classify, and rank material unknowns before implementation. Use for substantial, ambiguous, cross-module, architectural, security-sensitive, migration, synchronization, legacy-code, or prior-implementation review work where hidden constraints could change the solution; use a lightweight scan for small reversible changes.
---

# Unknowns Discovery

## Purpose

Discover the uncertainties, implicit expectations, hidden dependencies, and overlooked risks that could cause an implementation to solve the wrong problem or make unsafe decisions.

This skill is not a generic risk checklist. Every finding must be grounded in the current request, codebase, documentation, tests, data model, or observable project behavior.

## Trigger Conditions

Run this skill before implementation when a task involves one or more of the following:

- A new product feature or workflow
- Ambiguous product, UX, or visual requirements
- Architecture or data-model changes
- Database migrations or irreversible data operations
- Authentication, permissions, privacy, or security
- External APIs, synchronization, import, or export
- Cross-module or cross-platform changes
- Legacy code with unclear behavior
- A large change spanning multiple files or systems
- Requirements expressed through subjective terms such as "simple," "beautiful," "intelligent," "Apple-like," "natural," or "high quality"
- A prior implementation that technically worked but did not match the intended experience

For small, reversible, isolated changes, perform only a lightweight scan.

## Modes

### Lightweight

Use for local and reversible work.

Output only:

- Confirmed facts
- Material assumptions
- Immediate edge cases
- Remaining uncertainty

### Standard

Use for normal feature development.

Inspect the relevant code, tests, schema, documentation, and neighboring modules. Produce the complete Unknowns Report.

### Deep

Use for migrations, architecture, security, permissions, synchronization, major redesigns, or irreversible decisions.

In addition to the standard process:

- Inspect historical implementations and prior decisions
- Trace affected data and execution paths
- Generate competing solution models
- Perform a red-team review
- Define rollback and recovery requirements
- Require explicit evidence for high-impact conclusions

## Process

### 1. Restate the Intent

Describe:

- The user-visible problem
- The desired change in behavior
- The affected users or workflows
- The success criteria
- What is explicitly out of scope

Do not confuse the requested implementation with the underlying goal.

### 2. Gather Evidence

Inspect the most relevant available sources:

- Existing implementation
- Types and interfaces
- Database schema and migrations
- Tests
- Configuration
- API contracts
- Design system and comparable screens
- Project documentation
- Historical decisions or implementation notes
- Adjacent modules that may depend on the changed behavior

Separate verified evidence from inference.

### 3. Build the Unknowns Map

Classify findings as:

#### Known Knowns

Facts supported by direct evidence.

For each item include:

- Fact
- Evidence
- Relevance

#### Known Unknowns

Visible unresolved questions.

For each item include:

- Question
- Why it matters
- Options
- Recommended validation method

#### Unknown Knowns

Implicit expectations or knowledge that probably exists in the user, team, or current product but has not been stated.

Look especially for:

- Unwritten design taste
- Domain conventions
- Existing behavior users may rely on
- Implicit definitions of quality
- Decisions the requester can recognize but may struggle to describe abstractly

Convert these into prototypes, comparisons, examples, or focused decision questions.

#### Unknown Unknown Candidates

Search for overlooked:

- Hidden dependencies
- Compatibility constraints
- Data lifecycle issues
- Permission boundaries
- Failure and recovery paths
- Empty, loading, degraded, and offline states
- Concurrency and synchronization conflicts
- Scale limits
- Migration requirements
- Observability gaps
- Accessibility and localization effects
- Misuse and abuse cases
- Situations in which the current solution may be addressing the wrong problem

These are candidates, not proven facts. State the reasoning and validation method.

### 4. Rank the Unknowns

Score each material unknown from 1 to 5 on:

- Impact
- Probability
- Irreversibility
- Cost of late discovery

Use:

`Priority = Impact × Probability × Irreversibility × Late Discovery Cost`

Classify the result:

- **Blocker**: must be resolved before implementation
- **Decision**: requires an explicit product or architecture choice
- **Experiment**: should be tested through prototype, spike, or measurement
- **Monitor**: may proceed with instrumentation or follow-up
- **Accept**: low-risk uncertainty that can remain temporarily unresolved

Do not block work merely because an unknown exists.

### 5. Choose the Correct Resolution Method

Use the smallest reliable method:

- Inspect code or tests for factual uncertainty
- Run a technical spike for feasibility uncertainty
- Build contrasting prototypes for implicit UX or visual expectations
- Ask a focused decision question for product ambiguity
- Add a test for behavioral uncertainty
- Add instrumentation for production uncertainty
- Use a reversible implementation for low-risk unresolved choices
- Require explicit approval for irreversible or cross-system decisions

### 6. Produce the Unknowns Report

Use the template at:

`templates/unknowns-report.md`

The report must distinguish facts, inferences, assumptions, and proposals.

### 7. Implementation Handoff

Before implementation, convert resolved findings into:

- Acceptance criteria
- Data and interface contracts
- Explicit invariants
- Test cases
- Rollback requirements
- Observability requirements
- Non-goals

For substantial changes, create or update:

`templates/implementation-notes.md`

Never silently deviate from a material approved decision.

### 8. Post-Implementation Review

After implementation, use:

`templates/post-implementation-review.md`

Report:

- What system behavior changed
- Which assumptions were confirmed or disproved
- Which unknowns remain
- What deviations occurred
- How the result was verified
- How to roll it back
- What future maintainers must understand
- Which discoveries should become tests, documentation, or project conventions

For substantial changes, generate several scenario questions that verify the maintainer understands the new behavior.

## Quality Rules

- Do not generate generic enterprise risk lists without project evidence.
- Do not pretend unknown-unknown discovery is exhaustive.
- Do not ask questions that can be answered by inspecting the repository.
- Do not treat subjective requirements as implementation-ready.
- Do not resolve irreversible decisions through unmarked assumptions.
- Do not over-design low-impact and easily reversible work.
- Prefer evidence, prototypes, tests, and experiments over confident speculation.
- The goal is not to eliminate all uncertainty. The goal is to expose and govern the uncertainty that could materially change the result.

## Suggested Invocation

### Lightweight

```text
Run the Unknowns Discovery skill in lightweight mode for this change.
Inspect the relevant code first.
Do not produce generic risks.
```

### Standard

```text
Run the Unknowns Discovery skill in standard mode.
Do not implement yet.
Produce an Unknowns Report grounded in the repository.
```

### Deep

```text
Run the Unknowns Discovery skill in deep mode.

Focus on:
- architecture and data-model reversibility
- hidden dependencies
- migration and rollback
- implicit product and UX expectations
- evidence that the proposed solution may be solving the wrong problem

Do not change code before producing the report.
```
