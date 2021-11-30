# Developer notes

To avoid treading the same road twice.

## Avoiding copying the whole configuration
It is often desirable to not copy the entire config (say if it was really big).
Here are some notes on strategies to avoid this.

### Manually specifying files per deployment
e.g. like `deploy.nodes."<example>".files = ./example-files`.  

This must at least have both opt-in / opt-out semantics, i.e. "exclude all but
these files" and "include all but these files". Perhaps unions of that.  
This would make the `rsync` command a lot more complicated.

### `nix-instantiate` equivalent
Life would be a lot easier if we could use `nix-instantiate` and then copy the
intermediate form, but we can't because there's no equivalent for flakes
(https://github.com/NixOS/nix/issues/3908).

## Building the configuration on a different server than the target

### `nixos-rebuild`'s `--target-host` & `--build-host`

## Tests

The tests will probably unfortunately use Docker.

### Using `nixos-container`
We can't do this until https://github.com/NixOS/nixpkgs/pull/67336 is merged.

### Using `nixos-shell`
[nixos-shell](https://github.com/Mic92/nixos-shell) looks really cool, but it 
deploys to QEMU, which is not so nice - it takes way too long to start up.

## A predeploy command?
Like:
```nix
deploy = {
    predeploy = "nix eval --json -f sops.nix > .sops.yaml";
};
```
or something

## Show progress in some way
Currently there's no way to tell whether or not nixos-rebuild is downloading,
evaluating, or hanging. Using the flake progress bar would be nice.
