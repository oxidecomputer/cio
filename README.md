# cio

[![cargo-build](https://github.com/oxidecomputer/cio/workflows/cargo%20build/badge.svg)](https://github.com/oxidecomputer/cio/actions?query=workflow%3A%22cargo+build%22)
[![cargo-clippy](https://github.com/oxidecomputer/cio/workflows/cargo%20clippy/badge.svg)](https://github.com/oxidecomputer/cio/actions?query=workflow%3A%22cargo+clippy%22)
[![cargo-test](https://github.com/oxidecomputer/cio/workflows/cargo%20test/badge.svg)](https://github.com/oxidecomputer/cio/actions?query=workflow%3A%22cargo+test%22)
[![rustfmt](https://github.com/oxidecomputer/cio/workflows/rustfmt/badge.svg)](https://github.com/oxidecomputer/cio/actions?query=workflow%3A%22rustfmt%22)
[![cloud-run](https://github.com/oxidecomputer/cio/workflows/cloud-run/badge.svg)](https://github.com/oxidecomputer/cio/actions?query=workflow%3Acloud-run)

Helper functions and types for doing the activities of a CIO.

### Configuration

#### Runtime Flags

Specific runtime behaviors can be controlled via environment variables. Flags are disabled by default and setting the variable to `true` will enable the feature.

| Flag               | Description |
| ------------------ | ----------- |
| RFD_PDFS_IN_GITHUB | Enables committing of rendered RFD PDFs back to their source repo |
| RFD_PDFS_IN_GOOGLE_DRIVE | Enables writing of rendered RFD PDFs to Google Drive |

The architecture for this application server and all it's surroundings is:

![arch.png](arch.png)
