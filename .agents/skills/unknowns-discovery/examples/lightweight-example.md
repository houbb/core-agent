# Lightweight Example: Fix Empty-State Spacing

## Intent

Correct excessive vertical spacing in the search empty state without changing behavior, data flow, or responsive layout elsewhere.

## Confirmed Facts

- The spacing is controlled by the `SearchEmptyState` component.
- The component is used only by the global search page.
- No API, database, or state-management code is involved.
- Existing snapshot coverage includes the component structure but not computed spacing.

## Material Assumptions

- The current mobile spacing is intentional and should remain unchanged.
- Only desktop spacing should be adjusted.
- Existing typography and icon size are out of scope.

## Immediate Edge Cases

- Very short viewport heights
- Localized text wrapping to three or more lines
- Search page displayed inside a split-pane layout

## Remaining Uncertainty

- No visual-regression test currently verifies spacing.
- The exact accepted desktop spacing is inferred from adjacent empty-state components.

## Resolution

- Reuse the desktop spacing token already used by adjacent empty-state components.
- Preserve the existing mobile breakpoint.
- Add a targeted visual or component-level assertion if the project supports it.

## Implementation Boundary

Change only the component spacing class or token. Do not modify search behavior, copy, iconography, or breakpoints.
