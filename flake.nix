{
    description = "A Nix flake deployment tool";

    inputs = {
        utils.url = "github:numtide/flake-utils";
        naersk.url = "github:nmattia/naersk";
    };

    outputs = { self, nixpkgs, utils, naersk }: utils.lib.eachDefaultSystem (system: 
        let 
            naersk-lib = naersk.lib."${system}"; 
            pkgs = nixpkgs.legacyPackages."${system}";
        in {
            packages.henix = naersk-lib.buildPackage {
                pname = "henix";
                root = ./.;
                buildInputs = with pkgs; [
                    rsync
                ];
            };
            defaultPackage = self.packages."${system}".henix;

            devShell = pkgs.mkShell {
                inputsFrom = [ self.packages."${system}".henix ];
                buildInputs = with pkgs; [
                    cargo
                    rustc
                    nixUnstable
                    rust-analyzer
                    rustfmt
                    clippy
                ];
                # for rust-analyzer, see 
                # https://discourse.nixos.org/t/rust-src-not-found-and-other-misadventures-of-developing-rust-on-nixos/11570/5
                RUST_SRC_PATH = "${pkgs.rust.packages.stable.rustPlatform.rustLibSrc}";
            };
        }
    );
}
