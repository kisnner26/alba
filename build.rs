// el linker script define el mapa de memoria: si cambia, hay que reenlazar
fn main() {
    println!("cargo:rerun-if-changed=linker.ld");
}
