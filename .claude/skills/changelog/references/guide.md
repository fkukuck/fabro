# Changelog Writing Guide

How to write Arc changelog entries, inspired by [Linear's changelog](https://linear.app/changelog).

## Post structure

Each changelog entry follows this structure:

1. **Frontmatter** — title and date
2. **Breaking changes** — `<Warning>` blocks at the top (if any)
3. **Hero features** — 1-3 features with dedicated `##` sections
4. **Progressive disclosure footer** — `## More` heading with categorized one-liners inside `<Accordion>` groups

## Hero features

Each major feature gets its own `##` section. Write 2-4 sentences max. Most posts should have 2-4 hero features — be selective.

**A feature deserves a hero section if** it changes how users think about or interact with Arc — a new capability, a new integration, or a meaningful shift in workflow. New API endpoints, CLI commands, syntax additions, and incremental options belong in the accordion footer, not as hero sections.

**Combine closely related features** into a single hero section rather than splitting them. For example, three types of hooks (HTTP, prompt, agent) are one feature, not three.

**Lead with the problem or context, then present the solution:**

> Long-running agent sessions used to fail when they hit the context window limit. Now, sessions automatically summarize earlier conversation history when approaching the limit, so long workflows keep running without manual intervention.

**Use second person, present tense:** "You can now...", "Workflows now support..."

**Include a code example** when the feature has a CLI command, config snippet, or DOT syntax.

**Name features the way users know them.** Use the UI label or docs term, not internal module/crate names.

**Be specific about fixes.** "Fixes an issue where long-running stages could timeout during checkpoint saves" tells users whether this affected them. "Bug fixes" tells them nothing.

**Most important change first** — don't bury the lede.

## Progressive disclosure footer

After the hero features, add a `## More` heading followed by categorized one-liners inside `<Accordion>` components. This keeps the post scannable — readers who want details can expand the sections.

### Categories

Use only the categories that apply to a given post. Order them as listed:

| Category | What goes here | Verb tense |
|---|---|---|
| **API** | New/changed endpoints, query params, response shapes, server behavior changes | Present: "New `GET /usage` endpoint returns..." |
| **CLI** | New commands, flags, config, output formatting | Past: "Added `arc parse` command for inspecting workflow ASTs" |
| **Workflows** | DOT syntax, node types, stylesheet options, execution behavior | Past: "Added `wait.timer` node type for scheduled pauses" |
| **Fixes** | Bug fixes | Past: "Fixed UTF-8 slicing panic when..." |
| **Improvements** | Small enhancements, UI polish, perf wins | Past: "Added Gemini 3.1 Flash Lite to model catalog" |

### One-liner style

- Start with a verb (Added, Fixed, Improved, New, Updated)
- One line per item, no sub-bullets
- Use backticks for code: endpoints, flags, config keys, model names
- No periods at the end

## Title conventions

- Name the post after the hero feature: "Time in status", "Form templates"
- For multi-feature posts, list 2-3 top features: "mTLS auth, setup wizard, and arc doctor"
- No version numbers or dates in the title (the frontmatter has the date)

## What NOT to do

- Don't write more than 4 sentences per feature section
- Don't put fixes/improvements inline — they go in the accordion footer
- Don't promote every change to a hero section — new endpoints, flags, syntax options, and config knobs are accordion items unless they fundamentally change the user experience
- Don't split closely related features into separate hero sections — combine them
- Don't include changes that aren't meaningful to users (e.g., demo scaffolding, internal tooling)
- Don't use marketing superlatives ("revolutionary", "game-changing")
- Don't explain things the reader already knows — assume technical literacy
- Don't use internal names (Rust struct/module names, crate names, internal error types) — describe the behavior users see, not the code that changed. For example, "Fixed sandbox file-write validation incorrectly blocking valid operations" instead of "Fixed ReadBeforeWriteSandbox"
