# Security

Report suspected vulnerabilities to the maintainer privately through an available private contact or GitHub's private vulnerability reporting feature if enabled. Do not include exploit details, secrets, or personal data in a public issue before coordinating disclosure.

Version 0.0.4 verifies archive checksums and validates extraction paths but does not verify upstream package signatures. Use trusted HTTPS mirrors and treat repository indexes as part of the trust chain. See overview.md for the current security boundaries and limitations.

Conan catalog resolution executes upstream Python recipes with the user's permissions, including binary-only installations. Source builds also execute upstream build code. Tool SHA-256 pins, upstream revision locks, and cache manifests identify content; they do not certify a trusted publisher or safe code. Review catalogs, build tools, dependencies, and upstream sources. Conan owns package download/extraction behavior; cpuv's archive limits cover its MSYS2 and bootstrap paths only. Builds are not sandboxed or hermetic. The compiler fingerprint, saved profile, and binary inspection are compatibility evidence rather than a universal ABI guarantee. Cached hardlinked files must be treated as immutable.

Toolchain installation delegates explicit package requests to a discovered MSYS2 pacman host and uses its normal signature/script policy. This can modify that host installation and is not rolled back with a project environment. Compiler selection stores a verified local path without changing Windows PATH. Conan native commands can change the managed provider cache and remote configuration; recipe export/create executes upstream Python/build code. Keep credentials out of manifests and source control.
