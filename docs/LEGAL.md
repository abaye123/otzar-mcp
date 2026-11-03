# License constraints and approvals

The disk ships with `רישיון והסכם שימוש דיסק קשיח.pdf` (hard-disk license agreement). Relevant terms:

- Personal and private use only; no commercial use.
- Use only through the dedicated hardware; do not connect the database or any part of it
  to another computer or network, including the internet.
- Forbidden without permission: copying, distribution, reverse engineering, decompilation,
  disassembly, re-encoding, creating derivatives of the software, and removing copyright notices.
- The database is protected by technical measures.

## Rules this project follows because of the license

- Personal use on this machine only; stdio transport, no network listener.
- Read-only; no modification of the app or the disk.
- No bulk export: bounded results, page-text rate limits (docs/SPEC.md section 8).
- Encryption: do not break it. If page text requires decryption, get the method or a library
  from the publisher first.
- Respect subscription access when returning text.
