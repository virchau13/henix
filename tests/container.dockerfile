# vim:set ft=dockerfile:

FROM nixos/nix
RUN nix-channel --add https://nixos.org/channels/nixpkgs-unstable nixpkgs
RUN nix-channel --update