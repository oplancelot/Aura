fn main() {
    let session = ort::session::Session::builder()
        .unwrap()
        .commit_from_file("assets/silero_vad.onnx")
        .unwrap();

    println!("Inputs:");
    for input in session.inputs() {
        println!("- {}", input.name());
    }

    println!("Outputs:");
    for output in session.outputs() {
        println!("- {}", output.name());
    }
}
