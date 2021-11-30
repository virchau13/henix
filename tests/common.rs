use tokio::process::Command;

use rand::Rng;

/// Generates a random 32 bit number, and returns it as a 8-char hex string.
fn randhex32() -> String {
    let n: u32 = rand::thread_rng().gen();
    format!("{:08x}", n)
}

struct Container {
    name: String,
    ip: String
}

impl Container {
    // Creates a new container with flake support.
    // See the dev notes for why we use Docker instead of nixos-container.
    async fn new() -> Container {
        let name = randhex32();
        let create = Command::new("nixos-container")
            .arg("create")
            .arg(&name)
            .arg("--config")
            .arg(include_str!("container-cfg.nix"))
            .status()
            .await
            .unwrap();
        assert!(create.success());
        let out_ip = Command::new("nixos-container")
            .arg("show-ip")
            .arg(&name)
            .output()
            .await
            .unwrap();
    }
}

impl Drop for Container {
    fn drop(&mut self) {
        todo!()
    }
}
