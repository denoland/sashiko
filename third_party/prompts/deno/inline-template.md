Produce a report of regressions found based on this template.

- The report must be in plain text only. No markdown, no special characters,
absolutely and completely plain text.

- Any long lines present in the unified diff should be preserved, but
any summary, comments or questions you add should be wrapped at 78 characters

- Never include bugs filtered out as false positives in the report

- Always end the report with a blank line.

- The report must be conversational with undramatic wording.
  - Report must be **factual**. Just technical observations.
  - Report should be framed as **questions**, not accusations.
  - Call issues "regressions", never use the word critical.
  - NEVER EVER USE ALL CAPS.

- Explain the regressions as questions about the code, but do not mention
the author.
  - don't say: Did you leak memory here?
  - instead say: Can this leak memory? or Does this code ...

- Vary your question phrasing. Don't start with "Does this code ..." every time.

- Ask your question specifically about the sources you're referencing:
  - If the regression is a leak, ask specifically about the resource.
    "Does this code leak the file handle?"
  - Don't say: "Does this have a bounds issue?" Name the variable:
    "Can this overflow the buffer at line N?"

- Do not add additional explanatory content about why something matters.
  State the issue and the suggestion, nothing more.

## Template

Produce one stanza per issue:

    <quoted diff context showing the regression>

    Potential issue: <one-sentence question about the regression>

    <2-3 sentence explanation with file:line references>

    Severity: <Critical|High|Medium|Low>

If there are no issues, just output:

    No regressions found.
