// el mapa de memoria de los programas vive en user.ld: si cambia, hay que reenlazar
fn main() {
    println!("cargo:rerun-if-changed=user.ld");
}
