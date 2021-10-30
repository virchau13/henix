![henix logo](./docs/henix-logo-with-text.svg)
Henix is a simple NixOS flake deploy tool. It stores and builds the
configuration on the remote, allowing to deploy to servers of different
architectures. It can be run on any system that has the Nix package manager 
installed, and deploys to NixOS servers.

## Usage
Henix can be installed from the flake in this repository.

`henix deploy` deploys the configuration at the current directory to all
specified servers. As of right now, root SSH access is required.

Run `henix --help` for the full set of flags.

## The goals
- Be a simple NixOS deployment tool; deploy the flake, don't do much else.
- Be resistant; if something goes wrong, always have a rollback plan.
- Have good documentation, including examples, and good error messages.
- Be easily scriptable.

## How it works
When you run `henix deploy`, it computes the hash of the configuration directory 
using `nix-hash`, then copies the configuration to the server at the directory
`/etc/henix/{hash}`, e.g. `/etc/henix/4a8ff2c035228043c3dd2c017b6dca55`. 
In this way, Henix doesn't need to manage rollbacks on build failure; if the 
server build fails, the failing configuration is left at `/etc/henix/{hash}`, 
but otherwise nothing changes.

Other than that, there is no real magic here; Henix simply copies the specified
flake, then builds it using `nixos-rebuild --flake`.

## How does this compare to `deploy-rs`/`morph`/`nixops`/`nixus`/my favourite tool?
This is essentially just a simple deployment tool, since the Nix language is
powerful enough to construct any more complex deployments. Features that 
`deploy-rs`/`nixops`/etc. have that this does not have out-of-the-box (such as
multiple profiles or special module inputs) can be easily emulated by your
configuration or factored out into a flake library. 
<!-- (May want to add aquila-infra as an example here.) -->

The main benefit of this library is that it has better documentation.

## Planned features
- Magic rollback à la `deploy-rs`.
- A `clean` command to clean up old configurations.
- Secret management.
- `--dry-run`
- No need for `root` ssh access.
- Automated tests?
- Add examples and documentation.
