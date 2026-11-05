# Skills

Ycode supports the open **Agent Skills** format (https://agentskills.io/specification).

```
my-skill/
  SKILL.md        required: YAML frontmatter + Markdown instructions
  references/     optional docs the agent may read on demand
  assets/         optional templates / data
  scripts/        optional code — never run automatically
```

`SKILL.md`:

```markdown
---
name: release-notes
description: Draft release notes from a git log. Use when the user asks for release notes or a changelog.
---
# Steps
1. ...
```

Validation on install: `name` 1–64 chars of `a-z 0-9 -`, no leading/trailing or
double hyphens; `description` 1–1024 chars; `compatibility` ≤ 500 chars; folder
≤ 1000 files / 20 MB; symlinks are skipped. An example lives in
`examples/skills/release-notes`.

## Using skills

- **Install:** Settings → Skills → *Install from folder…* (copied into
  `%APPDATA%\com.ycode.desktop\skills`).
- **Enable / disable / delete** in the same list. Disabled skills are invisible to the model.
- **Automatic:** the model sees only each enabled skill's name and description and
  calls `load_skill` when one fits the task.
- **Explicit:** write `$release-notes` in a message to attach that skill's instructions
  to the message directly.

## Progressive disclosure (token & cache friendly)

| Stage | What enters the context | How |
|---|---|---|
| Start | name + description of enabled skills, sorted | one system block |
| Activation | `SKILL.md` body + list of its files | `load_skill` tool result |
| Details | one reference/asset text file | `read_skill_file` tool result |

Skill bodies arrive as tool results appended to the conversation, so they never
rewrite the cached system prefix. Enabling/disabling a skill changes the index block
and therefore costs one cache miss on the next request.

## Safety

- Skill text is labelled as third-party content; it cannot change Ycode's rules or
  approvals and has no access to API keys.
- `read_skill_file` is confined to the skill's folder (path traversal is rejected).
- Scripts run only through the Coding-mode terminal tool, which always asks the user
  first. `allowed-tools` in the frontmatter is ignored on purpose.
