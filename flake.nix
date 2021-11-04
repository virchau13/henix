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
                    nixUnstable
                    rust-analyzer
                    rustfmt
                    clippy
                ];
            };
        }
    );
}
