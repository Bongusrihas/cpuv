# Security

Report suspected vulnerabilities to the maintainer privately through an available private contact or GitHub's private vulnerability reporting feature if enabled. Do not include exploit details, secrets, or personal data in a public issue before coordinating disclosure.

Version 0.0.1 verifies archive checksums and validates extraction paths but does not verify upstream package signatures. Use trusted HTTPS mirrors and treat repository indexes as part of the trust chain. See overview.md for the current security boundaries and limitations.

Source builds execute upstream CMake/build code with the user's permissions. Recipe and tool SHA-256 pins identify expected content; they do not certify a trusted publisher or safe code. Review custom recipes, build tools, dependencies, and upstream sources. Builds are not sandboxed or hermetic. The compiler fingerprint, saved profile, and binary inspection are compatibility evidence rather than a universal ABI guarantee. Cached hardlinked files must be treated as immutable.
