# Project preferences

- Keep documentation condensed and practical: what the app does, how to install,
  use and build it. Avoid promotional prose, exhaustive feature inventories,
  implementation history and lengthy architecture explanations unless requested.
- Give each feature a declared Rust module with clear domain ownership. Keep
  Linux, macOS and Windows implementations adjacent behind matching interfaces.
- Treat each split as a tab group. Target tab actions at their owning group,
  with explicit identities and guards against stale workspace or machine state.
- Follow the existing visual language and shared components. Verify changed
  interactions, alignment and contrast in the UI; provide requested QA screenshots.
- When Markdown preview QA is requested, open the file in Zed on Linux and run
  the Markdown preview action before capturing its rendered display.
- Do not commit, tag or publish unless the user requests it for the current work.
