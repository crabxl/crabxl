# Security policy

## Supported versions

CrabXL is alpha software. Only the latest release receives security fixes.

## Report a vulnerability

Do not report a vulnerability in a public issue.

Use GitHub private vulnerability reporting:

1. Go to the [Security tab](https://github.com/crabxl/crabxl/security) of the
   repository.
2. Select **Report a vulnerability**.
3. Give the CrabXL version, the steps that cause the problem and, if possible, a
   file that shows it.

We will reply in the advisory. When a fix is available, we publish a release
and the advisory.

## Scope

CrabXL reads files that can come from untrusted sources. These problems are in
scope:

- A panic, crash or infinite loop caused by an input file.
- Memory or disk use that is larger than the configured limits.
- Temporary files that are not removed or that other users can read.
- Path traversal or other access outside the expected files.
